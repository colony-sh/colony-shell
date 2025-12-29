use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};

pub struct ProcessLineage {
    parent_map: RwLock<HashMap<u32, u32>>,
    approvals: RwLock<HashMap<u32, HashSet<String>>>,
}

impl ProcessLineage {
    pub fn new() -> Self {
        Self {
            parent_map: RwLock::new(HashMap::new()),
            approvals: RwLock::new(HashMap::new()),
        }
    }

    pub fn register_child(&self, parent_pid: u32, child_pid: u32) {
        self.parent_map.write().insert(child_pid, parent_pid);
    }

    pub fn grant_approval(&self, pid: u32, variable: &str) {
        self.approvals
            .write()
            .entry(pid)
            .or_default()
            .insert(variable.to_string());
    }

    pub fn has_approval(&self, pid: u32, variable: &str) -> bool {
        self.approvals
            .read()
            .get(&pid)
            .map(|vars| vars.contains(variable))
            .unwrap_or(false)
    }

    pub fn has_inherited_approval(&self, pid: u32, variable: &str) -> bool {
        let mut current = pid;
        let parent_map = self.parent_map.read();
        let approvals = self.approvals.read();

        loop {
            if let Some(vars) = approvals.get(&current) {
                if vars.contains(variable) {
                    return true;
                }
            }

            match parent_map.get(&current) {
                Some(&parent) if parent != current => {
                    current = parent;
                }
                _ => break,
            }
        }

        false
    }

    pub fn cleanup_process(&self, pid: u32) {
        self.parent_map.write().remove(&pid);
        self.approvals.write().remove(&pid);
    }

    pub fn get_ancestors(&self, pid: u32) -> Vec<u32> {
        let mut ancestors = Vec::new();
        let mut current = pid;
        let parent_map = self.parent_map.read();

        while let Some(&parent) = parent_map.get(&current) {
            if parent == current {
                break;
            }
            ancestors.push(parent);
            current = parent;
        }

        ancestors
    }
}

impl Default for ProcessLineage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_approval() {
        let lineage = ProcessLineage::new();

        assert!(!lineage.has_approval(1000, "SECRET"));

        lineage.grant_approval(1000, "SECRET");
        assert!(lineage.has_approval(1000, "SECRET"));
        assert!(!lineage.has_approval(1000, "OTHER"));
    }

    #[test]
    fn test_inherited_approval() {
        let lineage = ProcessLineage::new();

        lineage.grant_approval(1000, "AWS_KEY");
        lineage.register_child(1000, 1001);
        lineage.register_child(1001, 1002);

        assert!(lineage.has_inherited_approval(1000, "AWS_KEY"));
        assert!(lineage.has_inherited_approval(1001, "AWS_KEY"));
        assert!(lineage.has_inherited_approval(1002, "AWS_KEY"));

        assert!(!lineage.has_inherited_approval(1003, "AWS_KEY"));
    }

    #[test]
    fn test_get_ancestors() {
        let lineage = ProcessLineage::new();

        lineage.register_child(100, 200);
        lineage.register_child(200, 300);
        lineage.register_child(300, 400);

        let ancestors = lineage.get_ancestors(400);
        assert_eq!(ancestors, vec![300, 200, 100]);
    }

    #[test]
    fn test_cleanup_process() {
        let lineage = ProcessLineage::new();

        lineage.grant_approval(1000, "SECRET");
        lineage.register_child(1000, 1001);

        assert!(lineage.has_approval(1000, "SECRET"));

        lineage.cleanup_process(1000);

        assert!(!lineage.has_approval(1000, "SECRET"));
    }
}
