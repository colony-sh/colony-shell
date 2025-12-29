//! Linda-style tuple space for agent coordination
//!
//! Implements a shared memory coordination system based on the Linda model.
//! Tuples are ordered collections of heterogeneous values that can be
//! written to and read from a shared space.
//!
//! ## Linda Primitives
//!
//! - `out(tuple)` - Write tuple to space (non-blocking)
//! - `in(pattern)` - Read and remove matching tuple (blocking)
//! - `rd(pattern)` - Read matching tuple without removing (blocking)
//! - `inp(pattern)` - Non-blocking `in` (returns None if not found)
//! - `rdp(pattern)` - Non-blocking `rd` (returns None if not found)
//!
//! ## Storage Backends
//!
//! - `TupleSpace` - In-memory only (ephemeral, single process)
//! - `PersistentTupleSpace` - SQLite-backed (persistent, multi-process)
//!
//! ## Example
//!
//! ```rust
//! use colony_tuple::{TupleSpace, Tuple, Value};
//!
//! let space = TupleSpace::new();
//!
//! // Write a tuple
//! space.out(Tuple::from(vec![
//!     Value::Str("env_change".to_string()),
//!     Value::Str("PATH".to_string()),
//!     Value::Str("/usr/bin".to_string()),
//! ]));
//!
//! // Read it back (non-blocking)
//! let pattern = Tuple::pattern(&[
//!     Value::Str("env_change".to_string()),
//!     Value::Wildcard,
//!     Value::Wildcard,
//! ]);
//!
//! if let Some(tuple) = space.inp(pattern) {
//!     println!("Found tuple: {:?}", tuple);
//! }
//! ```
//!
//! ## Use Cases
//!
//! - **Reactive Environment**: `out("env_change", var, old, new)`
//! - **Agent Coordination**: `out("task", agent_id, work_item)`
//! - **Event Notification**: `out("file_change", path, mtime)`
//! - **Request/Response**: `out("request", id, data)` → `in("response", id, *)`

mod ipc;
mod persistent;

pub use ipc::{IpcError, IpcTupleSpace, TupleNotification};
pub use persistent::{PersistentTupleSpace, StorageError};

use parking_lot::{Condvar, Mutex};
use scc::HashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// A value that can be stored in a tuple
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// String value
    Str(String),
    /// Integer value
    Int(i64),
    /// Float value
    Float(f64),
    /// Boolean value
    Bool(bool),
    /// Null/None value
    Null,
    /// Wildcard - matches any value in a pattern
    Wildcard,
}

impl Value {
    /// Check if this value matches a pattern value
    pub fn matches(&self, pattern: &Value) -> bool {
        match pattern {
            Value::Wildcard => true,
            _ => self == pattern,
        }
    }
}

/// A tuple is an ordered sequence of values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tuple {
    pub fields: Vec<Value>,
}

impl Tuple {
    /// Create a new tuple
    pub fn new(fields: Vec<Value>) -> Self {
        Self { fields }
    }

    /// Create a pattern tuple (may contain wildcards)
    pub fn pattern(fields: &[Value]) -> Self {
        Self {
            fields: fields.to_vec(),
        }
    }

    /// Check if this tuple matches a pattern
    pub fn matches(&self, pattern: &Tuple) -> bool {
        if self.fields.len() != pattern.fields.len() {
            return false;
        }

        self.fields
            .iter()
            .zip(pattern.fields.iter())
            .all(|(val, pat)| val.matches(pat))
    }

    /// Get the number of fields
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Check if tuple is empty
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

impl From<Vec<Value>> for Tuple {
    fn from(fields: Vec<Value>) -> Self {
        Self::new(fields)
    }
}

/// Tuple space - shared coordination memory
///
/// Uses scc::HashMap for concurrent access (Rule #8) and parking_lot
/// for efficient blocking operations.
pub struct TupleSpace {
    /// Storage for tuples (keyed by tuple hash for fast lookup)
    tuples: Arc<HashMap<u64, Vec<Tuple>>>,
    /// Notification system for blocking operations
    notify: Arc<(Mutex<()>, Condvar)>,
    /// Counter for generating unique IDs
    next_id: Arc<Mutex<u64>>,
}

impl TupleSpace {
    /// Create a new tuple space
    pub fn new() -> Self {
        Self {
            tuples: Arc::new(HashMap::new()),
            notify: Arc::new((Mutex::new(()), Condvar::new())),
            next_id: Arc::new(Mutex::new(0)),
        }
    }

    /// out(tuple) - Write a tuple to the space (non-blocking)
    ///
    /// # Example
    ///
    /// ```rust
    /// use colony_tuple::{TupleSpace, Tuple, Value};
    ///
    /// let space = TupleSpace::new();
    /// space.out(Tuple::from(vec![
    ///     Value::Str("hello".to_string()),
    ///     Value::Int(42),
    /// ]));
    /// ```
    pub fn out(&self, tuple: Tuple) {
        // Use tuple length as a simple hash for bucketing
        // In a real implementation, we'd use a proper hash function
        let hash = tuple.len() as u64;

        // Add to the appropriate bucket
        self.tuples
            .entry(hash)
            .or_insert_with(Vec::new)
            .get_mut()
            .push(tuple);

        // Notify any waiting threads
        let (_lock, condvar) = &*self.notify;
        condvar.notify_all();
    }

    /// inp(pattern) - Non-blocking in (read and remove)
    ///
    /// Returns the first matching tuple, or None if no match found.
    ///
    /// # Example
    ///
    /// ```rust
    /// use colony_tuple::{TupleSpace, Tuple, Value};
    ///
    /// let space = TupleSpace::new();
    /// space.out(Tuple::from(vec![Value::Str("test".to_string())]));
    ///
    /// let pattern = Tuple::pattern(&[Value::Wildcard]);
    /// let result = space.inp(pattern);
    /// assert!(result.is_some());
    /// ```
    pub fn inp(&self, pattern: Tuple) -> Option<Tuple> {
        let hash = pattern.len() as u64;

        // Search the appropriate bucket
        if let Some(mut bucket) = self.tuples.get(&hash) {
            let bucket = bucket.get_mut();

            // Find first matching tuple
            if let Some(pos) = bucket.iter().position(|t| t.matches(&pattern)) {
                return Some(bucket.remove(pos));
            }
        }

        None
    }

    /// rdp(pattern) - Non-blocking rd (read without removing)
    ///
    /// Returns a clone of the first matching tuple, or None if no match found.
    ///
    /// # Example
    ///
    /// ```rust
    /// use colony_tuple::{TupleSpace, Tuple, Value};
    ///
    /// let space = TupleSpace::new();
    /// space.out(Tuple::from(vec![Value::Int(42)]));
    ///
    /// let pattern = Tuple::pattern(&[Value::Wildcard]);
    /// let result = space.rdp(pattern.clone());
    /// assert!(result.is_some());
    ///
    /// // Tuple still in space
    /// let result2 = space.rdp(pattern);
    /// assert!(result2.is_some());
    /// ```
    pub fn rdp(&self, pattern: Tuple) -> Option<Tuple> {
        let hash = pattern.len() as u64;

        // Search the appropriate bucket
        if let Some(bucket) = self.tuples.read(&hash, |_, tuples| {
            tuples.iter().find(|t| t.matches(&pattern)).cloned()
        }) {
            return bucket;
        }

        None
    }

    /// in(pattern) - Blocking read and remove
    ///
    /// Blocks until a matching tuple is found, then removes and returns it.
    ///
    /// # Example
    ///
    /// ```rust
    /// use colony_tuple::{TupleSpace, Tuple, Value};
    /// use std::thread;
    /// use std::time::Duration;
    ///
    /// let space = TupleSpace::new();
    /// let space_clone = space.clone();
    ///
    /// // Writer thread
    /// thread::spawn(move || {
    ///     thread::sleep(Duration::from_millis(10));
    ///     space_clone.out(Tuple::from(vec![Value::Str("ready".to_string())]));
    /// });
    ///
    /// // Reader blocks until tuple arrives
    /// let pattern = Tuple::pattern(&[Value::Str("ready".to_string())]);
    /// let result = space.r#in(pattern);
    /// assert!(result.is_some());
    /// ```
    pub fn r#in(&self, pattern: Tuple) -> Option<Tuple> {
        loop {
            // Try non-blocking first
            if let Some(tuple) = self.inp(pattern.clone()) {
                return Some(tuple);
            }

            // Wait for notification
            let (lock, condvar) = &*self.notify;
            let mut guard = lock.lock();
            condvar.wait_for(&mut guard, Duration::from_millis(100));
        }
    }

    /// rd(pattern) - Blocking read (without removing)
    ///
    /// Blocks until a matching tuple is found, then returns a clone.
    ///
    /// # Example
    ///
    /// ```rust
    /// use colony_tuple::{TupleSpace, Tuple, Value};
    ///
    /// let space = TupleSpace::new();
    /// space.out(Tuple::from(vec![Value::Int(123)]));
    ///
    /// let pattern = Tuple::pattern(&[Value::Wildcard]);
    /// let result = space.rd(pattern);
    /// assert!(result.is_some());
    ///
    /// // Tuple still there
    /// let result2 = space.rdp(Tuple::pattern(&[Value::Wildcard]));
    /// assert!(result2.is_some());
    /// ```
    pub fn rd(&self, pattern: Tuple) -> Option<Tuple> {
        loop {
            // Try non-blocking first
            if let Some(tuple) = self.rdp(pattern.clone()) {
                return Some(tuple);
            }

            // Wait for notification
            let (lock, condvar) = &*self.notify;
            let mut guard = lock.lock();
            condvar.wait_for(&mut guard, Duration::from_millis(100));
        }
    }

    /// Count tuples in the space
    pub fn len(&self) -> usize {
        let mut count = 0;
        self.tuples.scan(|_, bucket| {
            count += bucket.len();
        });
        count
    }

    /// Check if space is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Clone for TupleSpace {
    fn clone(&self) -> Self {
        Self {
            tuples: Arc::clone(&self.tuples),
            notify: Arc::clone(&self.notify),
            next_id: Arc::clone(&self.next_id),
        }
    }
}

impl Default for TupleSpace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_out_and_inp() {
        let space = TupleSpace::new();

        // Write a tuple
        space.out(Tuple::from(vec![
            Value::Str("test".to_string()),
            Value::Int(42),
        ]));

        // Read it back with exact match
        let pattern = Tuple::pattern(&[Value::Str("test".to_string()), Value::Int(42)]);
        let result = space.inp(pattern);

        assert!(result.is_some());
        let tuple = result.unwrap();
        assert_eq!(tuple.fields.len(), 2);
        assert_eq!(tuple.fields[0], Value::Str("test".to_string()));
        assert_eq!(tuple.fields[1], Value::Int(42));

        // Tuple should be gone
        assert_eq!(space.len(), 0);
    }

    #[test]
    fn test_wildcard_matching() {
        let space = TupleSpace::new();

        space.out(Tuple::from(vec![
            Value::Str("event".to_string()),
            Value::Str("file_changed".to_string()),
            Value::Str("/path/to/file".to_string()),
        ]));

        // Match with wildcards
        let pattern = Tuple::pattern(&[
            Value::Str("event".to_string()),
            Value::Wildcard, // Any event type
            Value::Wildcard, // Any path
        ]);

        let result = space.inp(pattern);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().fields[1],
            Value::Str("file_changed".to_string())
        );
    }

    #[test]
    fn test_rdp_non_destructive() {
        let space = TupleSpace::new();

        space.out(Tuple::from(vec![Value::Int(100)]));

        let pattern = Tuple::pattern(&[Value::Wildcard]);

        // First read
        let result1 = space.rdp(pattern.clone());
        assert!(result1.is_some());

        // Tuple should still be there
        let result2 = space.rdp(pattern);
        assert!(result2.is_some());

        assert_eq!(space.len(), 1);
    }

    #[test]
    fn test_no_match() {
        let space = TupleSpace::new();

        space.out(Tuple::from(vec![Value::Str("foo".to_string())]));

        let pattern = Tuple::pattern(&[Value::Str("bar".to_string())]);
        let result = space.inp(pattern);

        assert!(result.is_none());
        assert_eq!(space.len(), 1); // Original tuple still there
    }

    #[test]
    fn test_blocking_in() {
        let space = TupleSpace::new();
        let space_clone = space.clone();

        // Writer thread - writes after delay
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            space_clone.out(Tuple::from(vec![Value::Str("delayed".to_string())]));
        });

        // Reader blocks until tuple arrives
        let pattern = Tuple::pattern(&[Value::Str("delayed".to_string())]);
        let start = std::time::Instant::now();
        let result = space.r#in(pattern);

        assert!(result.is_some());
        assert!(start.elapsed() >= Duration::from_millis(40)); // Should have blocked
        assert_eq!(space.len(), 0); // Tuple was removed
    }

    #[test]
    fn test_blocking_rd() {
        let space = TupleSpace::new();
        let space_clone = space.clone();

        // Writer thread
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            space_clone.out(Tuple::from(vec![Value::Int(999)]));
        });

        // Reader blocks but doesn't remove
        let pattern = Tuple::pattern(&[Value::Wildcard]);
        let result = space.rd(pattern);

        assert!(result.is_some());
        assert_eq!(space.len(), 1); // Tuple still there
    }

    #[test]
    fn test_multiple_tuples() {
        let space = TupleSpace::new();

        // Write multiple tuples
        space.out(Tuple::from(vec![
            Value::Str("a".to_string()),
            Value::Int(1),
        ]));
        space.out(Tuple::from(vec![
            Value::Str("a".to_string()),
            Value::Int(2),
        ]));
        space.out(Tuple::from(vec![
            Value::Str("b".to_string()),
            Value::Int(3),
        ]));

        assert_eq!(space.len(), 3);

        // Match specific tuple
        let pattern = Tuple::pattern(&[Value::Str("a".to_string()), Value::Int(2)]);
        let result = space.inp(pattern);

        assert!(result.is_some());
        assert_eq!(result.unwrap().fields[1], Value::Int(2));
        assert_eq!(space.len(), 2);
    }

    #[test]
    fn test_reactive_environment_pattern() {
        let space = TupleSpace::new();

        // Simulate environment change notification
        space.out(Tuple::from(vec![
            Value::Str("env_change".to_string()),
            Value::Str("PATH".to_string()),
            Value::Str("/old/path".to_string()),
            Value::Str("/new/path".to_string()),
        ]));

        // Agent subscribes to env changes
        let pattern = Tuple::pattern(&[
            Value::Str("env_change".to_string()),
            Value::Wildcard, // Any variable
            Value::Wildcard, // Old value
            Value::Wildcard, // New value
        ]);

        let result = space.inp(pattern);
        assert!(result.is_some());

        let tuple = result.unwrap();
        assert_eq!(tuple.fields[1], Value::Str("PATH".to_string()));
        assert_eq!(tuple.fields[3], Value::Str("/new/path".to_string()));
    }
}
