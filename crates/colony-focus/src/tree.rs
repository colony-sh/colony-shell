//! Goal tree data structure
//!
//! Maintains parent/child relationships between goals with CRUD operations
//! and tree traversal utilities.

use std::collections::HashMap;

use crate::error::{FocusError, Result};
use crate::goal::{Goal, GoalStatus};

pub struct GoalTree {
    goals: HashMap<String, Goal>,
    root_ids: Vec<String>,
}

impl GoalTree {
    pub fn new() -> Self {
        Self {
            goals: HashMap::new(),
            root_ids: Vec::new(),
        }
    }

    pub fn add_root(&mut self, goal: Goal) -> Result<String> {
        if self.goals.contains_key(&goal.id) {
            return Err(FocusError::TreeIntegrity(format!(
                "goal '{}' already exists",
                goal.id
            )));
        }

        let id = goal.id.clone();
        if goal.parent_id.is_some() {
            return Err(FocusError::TreeIntegrity(
                "root goal cannot have a parent".to_string(),
            ));
        }
        self.root_ids.push(id.clone());
        self.goals.insert(id.clone(), goal);
        Ok(id)
    }

    pub fn add_child(&mut self, parent_id: &str, mut goal: Goal) -> Result<String> {
        if !self.goals.contains_key(parent_id) {
            return Err(FocusError::GoalNotFound(parent_id.to_string()));
        }

        if self.goals.contains_key(&goal.id) {
            return Err(FocusError::TreeIntegrity(format!(
                "goal '{}' already exists",
                goal.id
            )));
        }

        goal.parent_id = Some(parent_id.to_string());
        let id = goal.id.clone();

        self.goals.insert(id.clone(), goal);

        let parent = self.goals.get_mut(parent_id).expect("parent verified above");
        parent.children.push(id.clone());

        Ok(id)
    }

    pub fn get(&self, id: &str) -> Option<&Goal> {
        self.goals.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Goal> {
        self.goals.get_mut(id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.goals.contains_key(id)
    }

    pub fn remove(&mut self, id: &str) -> Result<Goal> {
        let goal = self
            .goals
            .get(id)
            .ok_or_else(|| FocusError::GoalNotFound(id.to_string()))?;

        if !goal.children.is_empty() {
            return Err(FocusError::TreeIntegrity(format!(
                "cannot remove goal '{}' with children; remove children first",
                id
            )));
        }

        let parent_id = goal.parent_id.clone();
        let goal = self.goals.remove(id).expect("verified above");

        if let Some(ref pid) = parent_id {
            if let Some(parent) = self.goals.get_mut(pid) {
                parent.children.retain(|c| c != id);
            }
        } else {
            self.root_ids.retain(|r| r != id);
        }

        Ok(goal)
    }

    pub fn transition(&mut self, id: &str, target: GoalStatus) -> Result<()> {
        let goal = self
            .goals
            .get_mut(id)
            .ok_or_else(|| FocusError::GoalNotFound(id.to_string()))?;
        goal.transition(target)
    }

    pub fn update_title(&mut self, id: &str, title: impl Into<String>) -> Result<()> {
        let goal = self
            .goals
            .get_mut(id)
            .ok_or_else(|| FocusError::GoalNotFound(id.to_string()))?;
        goal.title = title.into();
        Ok(())
    }

    pub fn update_description(
        &mut self,
        id: &str,
        description: Option<String>,
    ) -> Result<()> {
        let goal = self
            .goals
            .get_mut(id)
            .ok_or_else(|| FocusError::GoalNotFound(id.to_string()))?;
        goal.description = description;
        Ok(())
    }

    pub fn reparent(&mut self, id: &str, new_parent_id: Option<&str>) -> Result<()> {
        if !self.goals.contains_key(id) {
            return Err(FocusError::GoalNotFound(id.to_string()));
        }

        if let Some(new_pid) = new_parent_id {
            if !self.goals.contains_key(new_pid) {
                return Err(FocusError::GoalNotFound(new_pid.to_string()));
            }
            if new_pid == id {
                return Err(FocusError::TreeIntegrity(
                    "cannot reparent a goal to itself".to_string(),
                ));
            }
            let ancestors = self.get_ancestors(new_pid);
            if ancestors.iter().any(|a| a.id == id) {
                return Err(FocusError::TreeIntegrity(
                    "reparenting would create a cycle".to_string(),
                ));
            }
        }

        let old_parent_id = self.goals.get(id).expect("verified above").parent_id.clone();

        if let Some(ref old_pid) = old_parent_id {
            if let Some(old_parent) = self.goals.get_mut(old_pid) {
                old_parent.children.retain(|c| c != id);
            }
        } else {
            self.root_ids.retain(|r| r != id);
        }

        if let Some(new_pid) = new_parent_id {
            let new_parent = self.goals.get_mut(new_pid).expect("verified above");
            new_parent.children.push(id.to_string());

            let goal = self.goals.get_mut(id).expect("verified above");
            goal.parent_id = Some(new_pid.to_string());
        } else {
            let goal = self.goals.get_mut(id).expect("verified above");
            goal.parent_id = None;
            self.root_ids.push(id.to_string());
        }

        Ok(())
    }

    pub fn get_children(&self, id: &str) -> Vec<&Goal> {
        let Some(goal) = self.goals.get(id) else {
            return Vec::new();
        };

        goal.children
            .iter()
            .filter_map(|cid| self.goals.get(cid))
            .collect()
    }

    pub fn get_subtree(&self, id: &str) -> Vec<&Goal> {
        let mut result = Vec::new();
        self.collect_subtree(id, &mut result);
        result
    }

    fn collect_subtree<'a>(&'a self, id: &str, result: &mut Vec<&'a Goal>) {
        let Some(goal) = self.goals.get(id) else {
            return;
        };
        result.push(goal);
        for child_id in &goal.children {
            self.collect_subtree(child_id, result);
        }
    }

    pub fn get_ancestors(&self, id: &str) -> Vec<&Goal> {
        let mut result = Vec::new();
        let mut current = self.goals.get(id);

        while let Some(goal) = current {
            let Some(ref pid) = goal.parent_id else {
                break;
            };
            current = self.goals.get(pid);
            if let Some(parent) = current {
                result.push(parent);
            }
        }

        result
    }

    pub fn roots(&self) -> Vec<&Goal> {
        self.root_ids
            .iter()
            .filter_map(|id| self.goals.get(id))
            .collect()
    }

    pub fn all_goals(&self) -> Vec<&Goal> {
        self.goals.values().collect()
    }

    pub fn len(&self) -> usize {
        self.goals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.goals.is_empty()
    }

    pub fn active_goals(&self) -> Vec<&Goal> {
        self.goals
            .values()
            .filter(|g| g.is_active())
            .collect()
    }

    pub fn into_goals(self) -> Vec<Goal> {
        self.goals.into_values().collect()
    }

    pub fn from_goals(goals: Vec<Goal>) -> Result<Self> {
        let mut tree = Self::new();

        let goal_map: HashMap<String, Goal> = goals
            .into_iter()
            .map(|g| (g.id.clone(), g))
            .collect();

        for goal in goal_map.values() {
            if goal.parent_id.is_some() {
                continue;
            }
            tree.goals.insert(goal.id.clone(), goal.clone());
            tree.root_ids.push(goal.id.clone());
        }

        let mut remaining: Vec<&Goal> = goal_map
            .values()
            .filter(|g| g.parent_id.is_some())
            .collect();

        let mut last_len = remaining.len() + 1;
        while !remaining.is_empty() && remaining.len() < last_len {
            last_len = remaining.len();
            let mut still_remaining = Vec::new();

            for goal in remaining {
                let pid = goal.parent_id.as_deref().expect("filtered above");
                if tree.goals.contains_key(pid) {
                    tree.goals.insert(goal.id.clone(), goal.clone());
                    let parent = tree.goals.get_mut(pid).expect("checked above");
                    if !parent.children.contains(&goal.id) {
                        parent.children.push(goal.id.clone());
                    }
                } else {
                    still_remaining.push(goal);
                }
            }

            remaining = still_remaining;
        }

        if !remaining.is_empty() {
            let orphans: Vec<&str> = remaining.iter().map(|g| g.id.as_str()).collect();
            return Err(FocusError::TreeIntegrity(format!(
                "orphaned goals with missing parents: [{}]",
                orphans.join(", ")
            )));
        }

        Ok(tree)
    }
}

impl Default for GoalTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_goal(id: &str, title: &str) -> Goal {
        Goal::new(title).with_id(id)
    }

    #[test]
    fn test_add_root() {
        let mut tree = GoalTree::new();
        let id = tree.add_root(make_goal("g-1", "Root")).unwrap();
        assert_eq!(id, "g-1");
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.roots().len(), 1);
    }

    #[test]
    fn test_add_root_duplicate() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        assert!(tree.add_root(make_goal("g-1", "Dup")).is_err());
    }

    #[test]
    fn test_add_root_with_parent() {
        let mut tree = GoalTree::new();
        let goal = make_goal("g-1", "Root").with_parent("g-0");
        assert!(tree.add_root(goal).is_err());
    }

    #[test]
    fn test_add_child() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child")).unwrap();

        assert_eq!(tree.len(), 2);
        let root = tree.get("g-1").unwrap();
        assert_eq!(root.children, vec!["g-2"]);

        let child = tree.get("g-2").unwrap();
        assert_eq!(child.parent_id.as_deref(), Some("g-1"));
    }

    #[test]
    fn test_add_child_missing_parent() {
        let mut tree = GoalTree::new();
        assert!(tree
            .add_child("nonexistent", make_goal("g-1", "Child"))
            .is_err());
    }

    #[test]
    fn test_remove_leaf() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child")).unwrap();

        let removed = tree.remove("g-2").unwrap();
        assert_eq!(removed.id, "g-2");
        assert_eq!(tree.len(), 1);
        assert!(tree.get("g-1").unwrap().children.is_empty());
    }

    #[test]
    fn test_remove_with_children_fails() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child")).unwrap();

        assert!(tree.remove("g-1").is_err());
    }

    #[test]
    fn test_remove_root() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();

        tree.remove("g-1").unwrap();
        assert!(tree.is_empty());
        assert!(tree.roots().is_empty());
    }

    #[test]
    fn test_transition() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();

        tree.transition("g-1", GoalStatus::Completed).unwrap();
        assert_eq!(tree.get("g-1").unwrap().status, GoalStatus::Completed);
    }

    #[test]
    fn test_transition_invalid() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.transition("g-1", GoalStatus::Completed).unwrap();

        assert!(tree.transition("g-1", GoalStatus::Active).is_err());
    }

    #[test]
    fn test_transition_not_found() {
        let mut tree = GoalTree::new();
        assert!(tree.transition("nope", GoalStatus::Active).is_err());
    }

    #[test]
    fn test_update_title() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Old")).unwrap();
        tree.update_title("g-1", "New").unwrap();
        assert_eq!(tree.get("g-1").unwrap().title, "New");
    }

    #[test]
    fn test_update_description() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();

        tree.update_description("g-1", Some("A description".to_string()))
            .unwrap();
        assert_eq!(
            tree.get("g-1").unwrap().description.as_deref(),
            Some("A description")
        );

        tree.update_description("g-1", None).unwrap();
        assert!(tree.get("g-1").unwrap().description.is_none());
    }

    #[test]
    fn test_reparent() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root A")).unwrap();
        tree.add_root(make_goal("g-2", "Root B")).unwrap();
        tree.add_child("g-1", make_goal("g-3", "Child")).unwrap();

        tree.reparent("g-3", Some("g-2")).unwrap();

        assert!(tree.get("g-1").unwrap().children.is_empty());
        assert_eq!(tree.get("g-2").unwrap().children, vec!["g-3"]);
        assert_eq!(tree.get("g-3").unwrap().parent_id.as_deref(), Some("g-2"));
    }

    #[test]
    fn test_reparent_to_root() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child")).unwrap();

        tree.reparent("g-2", None).unwrap();

        assert!(tree.get("g-1").unwrap().children.is_empty());
        assert!(tree.get("g-2").unwrap().parent_id.is_none());
        assert_eq!(tree.roots().len(), 2);
    }

    #[test]
    fn test_reparent_self() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();

        assert!(tree.reparent("g-1", Some("g-1")).is_err());
    }

    #[test]
    fn test_reparent_cycle() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child")).unwrap();
        tree.add_child("g-2", make_goal("g-3", "Grandchild"))
            .unwrap();

        assert!(tree.reparent("g-1", Some("g-3")).is_err());
    }

    #[test]
    fn test_get_children() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child A")).unwrap();
        tree.add_child("g-1", make_goal("g-3", "Child B")).unwrap();

        let children = tree.get_children("g-1");
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_get_subtree() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child")).unwrap();
        tree.add_child("g-2", make_goal("g-3", "Grandchild"))
            .unwrap();
        tree.add_root(make_goal("g-4", "Other root")).unwrap();

        let subtree = tree.get_subtree("g-1");
        assert_eq!(subtree.len(), 3);
        assert_eq!(subtree[0].id, "g-1");
        assert_eq!(subtree[1].id, "g-2");
        assert_eq!(subtree[2].id, "g-3");
    }

    #[test]
    fn test_get_ancestors() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child")).unwrap();
        tree.add_child("g-2", make_goal("g-3", "Grandchild"))
            .unwrap();

        let ancestors = tree.get_ancestors("g-3");
        assert_eq!(ancestors.len(), 2);
        assert_eq!(ancestors[0].id, "g-2");
        assert_eq!(ancestors[1].id, "g-1");
    }

    #[test]
    fn test_get_ancestors_root() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();

        let ancestors = tree.get_ancestors("g-1");
        assert!(ancestors.is_empty());
    }

    #[test]
    fn test_active_goals() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Active")).unwrap();
        tree.add_root(make_goal("g-2", "Also active")).unwrap();
        tree.transition("g-2", GoalStatus::Completed).unwrap();

        let active = tree.active_goals();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "g-1");
    }

    #[test]
    fn test_from_goals() {
        let goals = vec![
            Goal::new("Root").with_id("g-1"),
            {
                let mut g = Goal::new("Child").with_id("g-2");
                g.parent_id = Some("g-1".to_string());
                g
            },
        ];

        let tree = GoalTree::from_goals(goals).unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.roots().len(), 1);
        assert_eq!(tree.get_children("g-1").len(), 1);
    }

    #[test]
    fn test_from_goals_orphan() {
        let goals = vec![{
            let mut g = Goal::new("Orphan").with_id("g-1");
            g.parent_id = Some("missing".to_string());
            g
        }];

        assert!(GoalTree::from_goals(goals).is_err());
    }

    #[test]
    fn test_into_goals() {
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child")).unwrap();

        let goals = tree.into_goals();
        assert_eq!(goals.len(), 2);
    }

    #[test]
    fn test_empty_tree() {
        let tree = GoalTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(tree.roots().is_empty());
        assert!(tree.all_goals().is_empty());
    }
}
