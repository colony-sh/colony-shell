//! Breakpoint management for the debug controller.

use serde::{Deserialize, Serialize};

/// A breakpoint definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub id: u32,
    pub line: u32,
    pub file: Option<String>,
    pub condition: Option<String>,
    pub enabled: bool,
    pub hit_count: u32,
}

/// Manages breakpoints for a debug session.
pub struct BreakpointManager {
    breakpoints: Vec<Breakpoint>,
    next_id: u32,
}

impl BreakpointManager {
    /// Create a new empty breakpoint manager.
    pub fn new() -> Self {
        Self {
            breakpoints: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a breakpoint at the given line, returning its ID.
    pub fn add(&mut self, line: u32, condition: Option<String>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.breakpoints.push(Breakpoint {
            id,
            line,
            file: None,
            condition,
            enabled: true,
            hit_count: 0,
        });
        id
    }

    /// Remove a breakpoint by ID. Returns true if it was found.
    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.breakpoints.len();
        self.breakpoints.retain(|b| b.id != id);
        self.breakpoints.len() != before
    }

    /// Toggle a breakpoint's enabled state.
    pub fn toggle(&mut self, id: u32) {
        if let Some(bp) = self.breakpoints.iter_mut().find(|b| b.id == id) {
            bp.enabled = !bp.enabled;
        }
    }

    /// Check if any enabled breakpoint matches the given line.
    /// Returns the breakpoint ID if hit.
    pub fn check(&mut self, line: u32) -> Option<u32> {
        for bp in &mut self.breakpoints {
            if bp.enabled && bp.line == line {
                bp.hit_count += 1;
                return Some(bp.id);
            }
        }
        None
    }

    /// List all breakpoints.
    pub fn list(&self) -> &[Breakpoint] {
        &self.breakpoints
    }

    /// Remove all breakpoints.
    pub fn clear(&mut self) {
        self.breakpoints.clear();
    }
}

impl Default for BreakpointManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_check() {
        let mut mgr = BreakpointManager::new();
        let id = mgr.add(10, None);
        assert_eq!(mgr.check(10), Some(id));
        assert_eq!(mgr.check(11), None);
    }

    #[test]
    fn disable_skips() {
        let mut mgr = BreakpointManager::new();
        let id = mgr.add(5, None);
        mgr.toggle(id);
        assert_eq!(mgr.check(5), None);
    }

    #[test]
    fn remove_works() {
        let mut mgr = BreakpointManager::new();
        let id = mgr.add(5, None);
        assert!(mgr.remove(id));
        assert_eq!(mgr.check(5), None);
    }

    #[test]
    fn hit_count_increments() {
        let mut mgr = BreakpointManager::new();
        mgr.add(5, None);
        mgr.check(5);
        mgr.check(5);
        assert_eq!(mgr.list()[0].hit_count, 2);
    }

    #[test]
    fn clear_removes_all() {
        let mut mgr = BreakpointManager::new();
        mgr.add(1, None);
        mgr.add(2, None);
        mgr.clear();
        assert!(mgr.list().is_empty());
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut mgr = BreakpointManager::new();
        assert!(!mgr.remove(42));
    }

    #[test]
    fn toggle_nonexistent_is_noop() {
        let mut mgr = BreakpointManager::new();
        mgr.toggle(42); // should not panic
    }

    #[test]
    fn re_enable_after_toggle() {
        let mut mgr = BreakpointManager::new();
        let id = mgr.add(5, None);
        mgr.toggle(id);
        assert_eq!(mgr.check(5), None);
        mgr.toggle(id);
        assert_eq!(mgr.check(5), Some(id));
    }

    #[test]
    fn conditional_breakpoint_stored() {
        let mut mgr = BreakpointManager::new();
        let id = mgr.add(10, Some("$x > 5".to_string()));
        let bp = mgr
            .list()
            .iter()
            .find(|b| b.id == id)
            .expect("breakpoint exists");
        assert_eq!(bp.condition.as_deref(), Some("$x > 5"));
    }
}
