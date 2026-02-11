//! Atomic multi-tuple transactions (P5-34)
//!
//! Provides a transaction type that batches multiple operations.
//! Atomic commit: all operations succeed or all roll back.
//! Supports write + remove in same transaction.
//! Includes deadlock detection for concurrent transactions.

use crate::{Tuple, TupleSpace};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tracing::debug;

/// Transaction ID for tracking and deadlock detection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionId(u64);

impl TransactionId {
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "txn-{}", self.0)
    }
}

/// Transaction errors
#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("transaction already committed")]
    AlreadyCommitted,

    #[error("transaction already rolled back")]
    AlreadyRolledBack,

    #[error("pattern not found during remove: no matching tuple")]
    RemoveNotFound,

    #[error("deadlock detected: transaction {0} would block on {1}")]
    DeadlockDetected(TransactionId, TransactionId),

    #[error("transaction lock timeout")]
    LockTimeout,
}

/// An operation within a transaction
#[derive(Debug, Clone)]
pub enum TxOp {
    /// Write a tuple to the space
    Write(Tuple),
    /// Remove a matching tuple from the space
    Remove(Tuple),
}

/// Transaction state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxState {
    Active,
    Committed,
    RolledBack,
}

/// A transaction that batches multiple tuple operations
///
/// Operations are collected and then atomically committed.
/// On commit, all writes and removes happen together.
/// On rollback, nothing is applied.
pub struct Transaction {
    id: TransactionId,
    state: TxState,
    operations: Vec<TxOp>,
}

impl Transaction {
    /// Create a new transaction
    fn new(id: TransactionId) -> Self {
        Self {
            id,
            state: TxState::Active,
            operations: Vec::new(),
        }
    }

    /// Get the transaction ID
    pub fn id(&self) -> TransactionId {
        self.id
    }

    /// Get the current state
    pub fn state(&self) -> TxState {
        self.state
    }

    /// Add a write operation to the transaction
    pub fn write(&mut self, tuple: Tuple) -> Result<(), TransactionError> {
        self.ensure_active()?;
        self.operations.push(TxOp::Write(tuple));
        Ok(())
    }

    /// Add a remove operation to the transaction
    pub fn remove(&mut self, pattern: Tuple) -> Result<(), TransactionError> {
        self.ensure_active()?;
        self.operations.push(TxOp::Remove(pattern));
        Ok(())
    }

    /// Get the pending operations
    pub fn operations(&self) -> &[TxOp] {
        &self.operations
    }

    fn ensure_active(&self) -> Result<(), TransactionError> {
        match self.state {
            TxState::Active => Ok(()),
            TxState::Committed => Err(TransactionError::AlreadyCommitted),
            TxState::RolledBack => Err(TransactionError::AlreadyRolledBack),
        }
    }
}

/// Manages transactions against a TupleSpace
pub struct TransactionManager {
    space: TupleSpace,
    next_id: AtomicU64,
    active_txns: Arc<Mutex<HashSet<u64>>>,
}

impl TransactionManager {
    /// Create a new transaction manager for a tuple space
    pub fn new(space: TupleSpace) -> Self {
        Self {
            space,
            next_id: AtomicU64::new(1),
            active_txns: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Begin a new transaction
    pub fn begin(&self) -> Transaction {
        let id_val = self.next_id.fetch_add(1, Ordering::SeqCst);
        let id = TransactionId(id_val);
        self.active_txns.lock().insert(id_val);
        debug!(txn = %id, "Transaction started");
        Transaction::new(id)
    }

    /// Commit a transaction atomically
    ///
    /// All operations are applied in order. If a remove operation
    /// cannot find a matching tuple, the entire transaction is rolled back.
    pub fn commit(&self, txn: &mut Transaction) -> Result<(), TransactionError> {
        txn.ensure_active()?;

        // Phase 1: Validate all removes can be satisfied
        // Collect tuples that will be removed
        let mut to_remove: Vec<Tuple> = Vec::new();
        let mut to_write: Vec<Tuple> = Vec::new();

        for op in &txn.operations {
            match op {
                TxOp::Write(tuple) => to_write.push(tuple.clone()),
                TxOp::Remove(pattern) => {
                    // Check if pattern matches existing tuple or a tuple being written
                    let from_space = self.space.rdp(pattern.clone());
                    let from_writes = to_write.iter().position(|t| t.matches(pattern));

                    if from_space.is_some() {
                        to_remove.push(pattern.clone());
                    } else if let Some(idx) = from_writes {
                        // Remove from the pending writes instead
                        to_remove.push(to_write.remove(idx));
                    } else {
                        txn.state = TxState::RolledBack;
                        self.active_txns.lock().remove(&txn.id.0);
                        debug!(txn = %txn.id, "Transaction rolled back: remove not found");
                        return Err(TransactionError::RemoveNotFound);
                    }
                }
            }
        }

        // Phase 2: Apply all operations atomically
        // Removes first (to free space), then writes
        for pattern in &to_remove {
            self.space.inp(pattern.clone());
        }

        for tuple in to_write {
            self.space.out(tuple);
        }

        txn.state = TxState::Committed;
        self.active_txns.lock().remove(&txn.id.0);
        debug!(txn = %txn.id, "Transaction committed");

        Ok(())
    }

    /// Roll back a transaction (discard all operations)
    pub fn rollback(&self, txn: &mut Transaction) -> Result<(), TransactionError> {
        txn.ensure_active()?;
        txn.state = TxState::RolledBack;
        self.active_txns.lock().remove(&txn.id.0);
        debug!(txn = %txn.id, "Transaction rolled back");
        Ok(())
    }

    /// Get the number of active transactions (for deadlock detection)
    pub fn active_count(&self) -> usize {
        self.active_txns.lock().len()
    }

    /// Check for potential deadlock between two transactions
    ///
    /// Returns true if committing txn_a would block on resources held by txn_b
    /// and vice versa.
    pub fn check_deadlock(
        &self,
        txn_a: &Transaction,
        txn_b: &Transaction,
    ) -> bool {
        // Check if txn_a removes something txn_b writes, and vice versa
        let a_removes: Vec<&Tuple> = txn_a
            .operations
            .iter()
            .filter_map(|op| match op {
                TxOp::Remove(p) => Some(p),
                _ => None,
            })
            .collect();

        let b_removes: Vec<&Tuple> = txn_b
            .operations
            .iter()
            .filter_map(|op| match op {
                TxOp::Remove(p) => Some(p),
                _ => None,
            })
            .collect();

        let a_writes: Vec<&Tuple> = txn_a
            .operations
            .iter()
            .filter_map(|op| match op {
                TxOp::Write(t) => Some(t),
                _ => None,
            })
            .collect();

        let b_writes: Vec<&Tuple> = txn_b
            .operations
            .iter()
            .filter_map(|op| match op {
                TxOp::Write(t) => Some(t),
                _ => None,
            })
            .collect();

        // Deadlock: A removes what B writes AND B removes what A writes
        let a_blocks_b = a_removes
            .iter()
            .any(|pat| b_writes.iter().any(|t| t.matches(pat)));
        let b_blocks_a = b_removes
            .iter()
            .any(|pat| a_writes.iter().any(|t| t.matches(pat)));

        a_blocks_b && b_blocks_a
    }

    /// Get reference to the underlying tuple space
    pub fn space(&self) -> &TupleSpace {
        &self.space
    }
}

impl Clone for TransactionManager {
    fn clone(&self) -> Self {
        Self {
            space: self.space.clone(),
            next_id: AtomicU64::new(self.next_id.load(Ordering::SeqCst)),
            active_txns: Arc::clone(&self.active_txns),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    #[test]
    fn test_transaction_write_and_commit() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space.clone());

        let mut txn = mgr.begin();
        txn.write(Tuple::from(vec![Value::Str("a".to_string()), Value::Int(1)]))
            .unwrap();
        txn.write(Tuple::from(vec![Value::Str("b".to_string()), Value::Int(2)]))
            .unwrap();

        mgr.commit(&mut txn).unwrap();

        assert_eq!(txn.state(), TxState::Committed);
        assert_eq!(space.len(), 2);
    }

    #[test]
    fn test_transaction_rollback() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space.clone());

        let mut txn = mgr.begin();
        txn.write(Tuple::from(vec![Value::Int(1)])).unwrap();
        txn.write(Tuple::from(vec![Value::Int(2)])).unwrap();

        mgr.rollback(&mut txn).unwrap();

        assert_eq!(txn.state(), TxState::RolledBack);
        assert_eq!(space.len(), 0);
    }

    #[test]
    fn test_transaction_remove() {
        let space = TupleSpace::new();
        space.out(Tuple::from(vec![
            Value::Str("remove_me".to_string()),
            Value::Int(42),
        ]));
        assert_eq!(space.len(), 1);

        let mgr = TransactionManager::new(space.clone());

        let mut txn = mgr.begin();
        txn.remove(Tuple::pattern(&[
            Value::Str("remove_me".to_string()),
            Value::Wildcard,
        ]))
        .unwrap();

        mgr.commit(&mut txn).unwrap();
        assert_eq!(space.len(), 0);
    }

    #[test]
    fn test_transaction_write_and_remove() {
        let space = TupleSpace::new();
        space.out(Tuple::from(vec![
            Value::Str("old".to_string()),
            Value::Int(1),
        ]));

        let mgr = TransactionManager::new(space.clone());

        let mut txn = mgr.begin();
        txn.remove(Tuple::pattern(&[
            Value::Str("old".to_string()),
            Value::Wildcard,
        ]))
        .unwrap();
        txn.write(Tuple::from(vec![
            Value::Str("new".to_string()),
            Value::Int(2),
        ]))
        .unwrap();

        mgr.commit(&mut txn).unwrap();

        assert_eq!(space.len(), 1);
        let result = space.inp(Tuple::pattern(&[
            Value::Str("new".to_string()),
            Value::Wildcard,
        ]));
        assert!(result.is_some());
        assert_eq!(result.unwrap().fields[1], Value::Int(2));
    }

    #[test]
    fn test_transaction_remove_not_found_rolls_back() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space.clone());

        let mut txn = mgr.begin();
        txn.write(Tuple::from(vec![Value::Int(1)])).unwrap();
        txn.remove(Tuple::pattern(&[Value::Str("nonexistent".to_string())]))
            .unwrap();

        let result = mgr.commit(&mut txn);
        assert!(matches!(result, Err(TransactionError::RemoveNotFound)));
        assert_eq!(txn.state(), TxState::RolledBack);
        assert_eq!(space.len(), 0);
    }

    #[test]
    fn test_transaction_cannot_write_after_commit() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space);

        let mut txn = mgr.begin();
        mgr.commit(&mut txn).unwrap();

        let result = txn.write(Tuple::from(vec![Value::Int(1)]));
        assert!(matches!(result, Err(TransactionError::AlreadyCommitted)));
    }

    #[test]
    fn test_transaction_cannot_commit_after_rollback() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space);

        let mut txn = mgr.begin();
        mgr.rollback(&mut txn).unwrap();

        let result = mgr.commit(&mut txn);
        assert!(matches!(result, Err(TransactionError::AlreadyRolledBack)));
    }

    #[test]
    fn test_transaction_active_count() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space);

        assert_eq!(mgr.active_count(), 0);

        let mut txn1 = mgr.begin();
        assert_eq!(mgr.active_count(), 1);

        let mut txn2 = mgr.begin();
        assert_eq!(mgr.active_count(), 2);

        mgr.commit(&mut txn1).unwrap();
        assert_eq!(mgr.active_count(), 1);

        mgr.rollback(&mut txn2).unwrap();
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn test_deadlock_detection() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space);

        let mut txn_a = mgr.begin();
        txn_a.write(Tuple::from(vec![Value::Str("a".to_string())])).unwrap();
        txn_a
            .remove(Tuple::pattern(&[Value::Str("b".to_string())]))
            .unwrap();

        let mut txn_b = mgr.begin();
        txn_b.write(Tuple::from(vec![Value::Str("b".to_string())])).unwrap();
        txn_b
            .remove(Tuple::pattern(&[Value::Str("a".to_string())]))
            .unwrap();

        assert!(mgr.check_deadlock(&txn_a, &txn_b));
    }

    #[test]
    fn test_no_deadlock() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space);

        let mut txn_a = mgr.begin();
        txn_a.write(Tuple::from(vec![Value::Str("a".to_string())])).unwrap();

        let mut txn_b = mgr.begin();
        txn_b.write(Tuple::from(vec![Value::Str("b".to_string())])).unwrap();

        assert!(!mgr.check_deadlock(&txn_a, &txn_b));
    }

    #[test]
    fn test_transaction_id_display() {
        let id = TransactionId(42);
        assert_eq!(format!("{}", id), "txn-42");
    }

    #[test]
    fn test_transaction_write_then_remove_same_tuple() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space.clone());

        let mut txn = mgr.begin();
        txn.write(Tuple::from(vec![Value::Str("temp".to_string()), Value::Int(1)]))
            .unwrap();
        txn.remove(Tuple::pattern(&[
            Value::Str("temp".to_string()),
            Value::Wildcard,
        ]))
        .unwrap();

        mgr.commit(&mut txn).unwrap();

        // Wrote and then removed in same transaction -> net zero
        assert_eq!(space.len(), 0);
    }

    #[test]
    fn test_multiple_transactions_sequential() {
        let space = TupleSpace::new();
        let mgr = TransactionManager::new(space.clone());

        let mut txn1 = mgr.begin();
        txn1.write(Tuple::from(vec![Value::Int(1)])).unwrap();
        mgr.commit(&mut txn1).unwrap();

        let mut txn2 = mgr.begin();
        txn2.write(Tuple::from(vec![Value::Int(2)])).unwrap();
        txn2.remove(Tuple::pattern(&[Value::Int(1)])).unwrap();
        mgr.commit(&mut txn2).unwrap();

        assert_eq!(space.len(), 1);
        let result = space.inp(Tuple::pattern(&[Value::Wildcard]));
        assert_eq!(result.unwrap().fields[0], Value::Int(2));
    }
}
