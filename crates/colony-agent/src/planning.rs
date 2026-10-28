//! Multi-turn planning with goal tree integration (P5-36)
//!
//! Provides a `PlanningAgent` wrapper that uses `GoalTree` for structured task
//! decomposition. The agent can:
//! - Create and update goals in the `GoalTree` during planning
//! - Decompose a top-level task into sub-goals
//! - Execute each sub-goal and update its status
//! - Flow goal status updates back into the agent context

#[cfg(feature = "focus")]
mod inner {
    use crate::AgentError;
    use colony_focus::goal::{Goal, GoalStatus};
    use colony_focus::tree::GoalTree;
    use tracing::{debug, info};

    /// A sub-goal produced during planning decomposition
    #[derive(Debug, Clone)]
    pub struct SubGoal {
        pub goal_id: String,
        pub title: String,
        pub description: Option<String>,
    }

    /// Result of executing a single sub-goal
    #[derive(Debug, Clone)]
    pub enum SubGoalOutcome {
        Completed { output: String },
        Blocked { reason: String },
        Failed { error: String },
    }

    /// Summary of the entire plan execution
    #[derive(Debug, Clone)]
    pub struct PlanExecutionSummary {
        pub root_goal_id: String,
        pub total_sub_goals: usize,
        pub completed: usize,
        pub blocked: usize,
        pub failed: usize,
        pub outputs: Vec<(String, SubGoalOutcome)>,
    }

    impl PlanExecutionSummary {
        pub fn is_fully_completed(&self) -> bool {
            self.completed == self.total_sub_goals
        }

        pub fn context_summary(&self) -> String {
            let mut parts = Vec::new();
            parts.push(format!(
                "Plan {} — {}/{} sub-goals completed",
                self.root_goal_id, self.completed, self.total_sub_goals,
            ));

            for (goal_id, outcome) in &self.outputs {
                let status_str = match outcome {
                    SubGoalOutcome::Completed { output } => {
                        let truncated = if output.len() > 120 {
                            format!("{}...", &output[..120])
                        } else {
                            output.clone()
                        };
                        format!("  [done] {}: {}", goal_id, truncated)
                    }
                    SubGoalOutcome::Blocked { reason } => {
                        format!("  [blocked] {}: {}", goal_id, reason)
                    }
                    SubGoalOutcome::Failed { error } => {
                        format!("  [failed] {}: {}", goal_id, error)
                    }
                };
                parts.push(status_str);
            }

            parts.join("\n")
        }
    }

    /// Wrapper that adds goal-tree-based planning to agent processing
    ///
    /// The `PlanningAgent` holds a `GoalTree` and provides methods to:
    /// 1. Decompose a task into a root goal with sub-goals
    /// 2. Iterate through sub-goals, executing each in turn
    /// 3. Update goal statuses based on execution results
    /// 4. Build context strings reflecting current plan state
    pub struct PlanningAgent {
        tree: GoalTree,
    }

    impl PlanningAgent {
        pub fn new() -> Self {
            Self {
                tree: GoalTree::new(),
            }
        }

        pub fn with_tree(tree: GoalTree) -> Self {
            Self { tree }
        }

        /// Access the underlying goal tree
        pub fn tree(&self) -> &GoalTree {
            &self.tree
        }

        /// Mutably access the underlying goal tree
        pub fn tree_mut(&mut self) -> &mut GoalTree {
            &mut self.tree
        }

        /// Create a root goal for a task and decompose it into sub-goals
        ///
        /// Returns the root goal ID and the list of sub-goals created.
        pub fn create_plan(
            &mut self,
            task_title: &str,
            task_description: Option<&str>,
            sub_goal_titles: &[(&str, Option<&str>)],
        ) -> Result<(String, Vec<SubGoal>), AgentError> {
            let mut root = Goal::new(task_title);
            if let Some(desc) = task_description {
                root = root.with_description(desc);
            }

            let root_id = self
                .tree
                .add_root(root)
                .map_err(|e| AgentError::Protocol(format!("failed to create root goal: {}", e)))?;

            info!(root_id = %root_id, title = task_title, "Created planning root goal");

            let mut sub_goals = Vec::with_capacity(sub_goal_titles.len());

            for (title, description) in sub_goal_titles {
                let mut goal = Goal::new(*title);
                if let Some(desc) = description {
                    goal = goal.with_description(*desc);
                }

                let goal_id = self.tree.add_child(&root_id, goal).map_err(|e| {
                    AgentError::Protocol(format!("failed to create sub-goal: {}", e))
                })?;

                sub_goals.push(SubGoal {
                    goal_id,
                    title: title.to_string(),
                    description: description.map(|s| s.to_string()),
                });
            }

            info!(
                root_id = %root_id,
                sub_goal_count = sub_goals.len(),
                "Plan decomposed into sub-goals"
            );

            Ok((root_id, sub_goals))
        }

        /// Add a single sub-goal to an existing parent
        pub fn add_sub_goal(
            &mut self,
            parent_id: &str,
            title: &str,
            description: Option<&str>,
        ) -> Result<SubGoal, AgentError> {
            let mut goal = Goal::new(title);
            if let Some(desc) = description {
                goal = goal.with_description(desc);
            }

            let goal_id = self
                .tree
                .add_child(parent_id, goal)
                .map_err(|e| AgentError::Protocol(format!("failed to add sub-goal: {}", e)))?;

            Ok(SubGoal {
                goal_id,
                title: title.to_string(),
                description: description.map(|s| s.to_string()),
            })
        }

        /// Record the outcome of executing a sub-goal
        pub fn record_outcome(
            &mut self,
            goal_id: &str,
            outcome: &SubGoalOutcome,
        ) -> Result<(), AgentError> {
            let target_status = match outcome {
                SubGoalOutcome::Completed { .. } => GoalStatus::Completed,
                SubGoalOutcome::Blocked { .. } => GoalStatus::Blocked,
                SubGoalOutcome::Failed { .. } => GoalStatus::Cancelled,
            };

            self.tree.transition(goal_id, target_status).map_err(|e| {
                AgentError::Protocol(format!("failed to transition goal '{}': {}", goal_id, e))
            })?;

            debug!(
                goal_id = goal_id,
                status = target_status.as_str(),
                "Goal status updated"
            );

            Ok(())
        }

        /// Get the next pending (active) sub-goals for a root goal, in order
        pub fn pending_sub_goals(&self, root_id: &str) -> Vec<SubGoal> {
            self.tree
                .get_children(root_id)
                .into_iter()
                .filter(|g| g.is_active())
                .map(|g| SubGoal {
                    goal_id: g.id.clone(),
                    title: g.title.clone(),
                    description: g.description.clone(),
                })
                .collect()
        }

        /// Execute a plan by iterating through sub-goals
        ///
        /// Takes a closure that processes each sub-goal and returns an outcome.
        /// Updates goal statuses and builds a summary.
        pub fn execute_plan<F>(
            &mut self,
            root_id: &str,
            mut executor: F,
        ) -> Result<PlanExecutionSummary, AgentError>
        where
            F: FnMut(&SubGoal) -> SubGoalOutcome,
        {
            let sub_goals = self.pending_sub_goals(root_id);
            let total = sub_goals.len();
            let mut completed = 0usize;
            let mut blocked = 0usize;
            let mut failed = 0usize;
            let mut outputs = Vec::with_capacity(total);

            for sub_goal in &sub_goals {
                let outcome = executor(sub_goal);

                match &outcome {
                    SubGoalOutcome::Completed { .. } => completed += 1,
                    SubGoalOutcome::Blocked { .. } => blocked += 1,
                    SubGoalOutcome::Failed { .. } => failed += 1,
                }

                self.record_outcome(&sub_goal.goal_id, &outcome)?;
                outputs.push((sub_goal.goal_id.clone(), outcome));
            }

            // If all sub-goals completed, mark the root as completed too
            if completed == total && total > 0 {
                let _ = self.tree.transition(root_id, GoalStatus::Completed);
                info!(
                    root_id = root_id,
                    "All sub-goals completed, root goal completed"
                );
            }

            Ok(PlanExecutionSummary {
                root_goal_id: root_id.to_string(),
                total_sub_goals: total,
                completed,
                blocked,
                failed,
                outputs,
            })
        }

        /// Build a context string describing the current state of a plan
        ///
        /// This can be injected into the agent's system prompt or conversation
        /// to keep it aware of planning progress.
        pub fn build_plan_context(&self, root_id: &str) -> Option<String> {
            let root = self.tree.get(root_id)?;
            let children = self.tree.get_children(root_id);

            if children.is_empty() {
                return Some(format!(
                    "[Current goal: {} — status: {}]",
                    root.title,
                    root.status.as_str(),
                ));
            }

            let mut lines = Vec::with_capacity(children.len() + 2);
            lines.push(format!(
                "[Plan: {} — status: {}]",
                root.title,
                root.status.as_str(),
            ));

            for child in &children {
                let marker = match child.status {
                    GoalStatus::Active => "[ ]",
                    GoalStatus::Completed => "[x]",
                    GoalStatus::Blocked => "[!]",
                    GoalStatus::Cancelled => "[-]",
                };
                lines.push(format!("  {} {}", marker, child.title));
            }

            lines.push("[End plan]".to_string());
            Some(lines.join("\n"))
        }
    }

    impl Default for PlanningAgent {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(feature = "focus")]
pub use inner::*;

#[cfg(not(feature = "focus"))]
pub mod stub {
    #[derive(Debug, Clone, Default)]
    pub struct PlanningConfig {
        pub enabled: bool,
    }
}

#[cfg(not(feature = "focus"))]
pub use stub::*;

#[cfg(test)]
#[cfg(feature = "focus")]
mod tests {
    use super::*;

    #[test]
    fn test_create_plan() {
        let mut pa = PlanningAgent::new();

        let (root_id, sub_goals) = pa
            .create_plan(
                "Build feature X",
                Some("Implement the full feature"),
                &[
                    ("Design API", Some("Define endpoints")),
                    ("Implement backend", None),
                    ("Write tests", Some("Unit and integration")),
                ],
            )
            .expect("plan creation should succeed");

        assert_eq!(sub_goals.len(), 3);
        assert_eq!(pa.tree().len(), 4);

        let root = pa.tree().get(&root_id).expect("root should exist");
        assert_eq!(root.title, "Build feature X");
        assert_eq!(root.children.len(), 3);
    }

    #[test]
    fn test_pending_sub_goals() {
        let mut pa = PlanningAgent::new();

        let (root_id, sub_goals) = pa
            .create_plan("Task", None, &[("A", None), ("B", None), ("C", None)])
            .expect("plan creation");

        let pending = pa.pending_sub_goals(&root_id);
        assert_eq!(pending.len(), 3);

        pa.record_outcome(
            &sub_goals[0].goal_id,
            &SubGoalOutcome::Completed {
                output: "done".to_string(),
            },
        )
        .expect("record outcome");

        let pending = pa.pending_sub_goals(&root_id);
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_execute_plan_all_completed() {
        let mut pa = PlanningAgent::new();

        let (root_id, _) = pa
            .create_plan("Task", None, &[("A", None), ("B", None)])
            .expect("plan creation");

        let summary = pa
            .execute_plan(&root_id, |_sub_goal| SubGoalOutcome::Completed {
                output: "ok".to_string(),
            })
            .expect("execution");

        assert!(summary.is_fully_completed());
        assert_eq!(summary.completed, 2);
        assert_eq!(summary.blocked, 0);
        assert_eq!(summary.failed, 0);

        let root = pa.tree().get(&root_id).expect("root");
        assert!(root.is_completed());
    }

    #[test]
    fn test_execute_plan_with_blocked() {
        let mut pa = PlanningAgent::new();

        let (root_id, sub_goals) = pa
            .create_plan("Task", None, &[("A", None), ("B", None)])
            .expect("plan creation");

        let blocked_id = sub_goals[1].goal_id.clone();

        let summary = pa
            .execute_plan(&root_id, |sg| {
                if sg.goal_id == blocked_id {
                    SubGoalOutcome::Blocked {
                        reason: "waiting on dependency".to_string(),
                    }
                } else {
                    SubGoalOutcome::Completed {
                        output: "done".to_string(),
                    }
                }
            })
            .expect("execution");

        assert!(!summary.is_fully_completed());
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.blocked, 1);

        let root = pa.tree().get(&root_id).expect("root");
        assert!(root.is_active());
    }

    #[test]
    fn test_execute_plan_with_failure() {
        let mut pa = PlanningAgent::new();

        let (root_id, _) = pa
            .create_plan("Task", None, &[("A", None)])
            .expect("plan creation");

        let summary = pa
            .execute_plan(&root_id, |_| SubGoalOutcome::Failed {
                error: "compilation error".to_string(),
            })
            .expect("execution");

        assert_eq!(summary.failed, 1);
        assert_eq!(summary.completed, 0);
    }

    #[test]
    fn test_add_sub_goal() {
        let mut pa = PlanningAgent::new();

        let (root_id, _) = pa
            .create_plan("Task", None, &[("A", None)])
            .expect("plan creation");

        let new_goal = pa
            .add_sub_goal(&root_id, "B", Some("additional work"))
            .expect("add sub-goal");

        assert_eq!(new_goal.title, "B");
        assert_eq!(pa.pending_sub_goals(&root_id).len(), 2);
    }

    #[test]
    fn test_build_plan_context() {
        let mut pa = PlanningAgent::new();

        let (root_id, sub_goals) = pa
            .create_plan(
                "Build feature",
                None,
                &[("Design", None), ("Implement", None), ("Test", None)],
            )
            .expect("plan creation");

        pa.record_outcome(
            &sub_goals[0].goal_id,
            &SubGoalOutcome::Completed {
                output: "designed".to_string(),
            },
        )
        .expect("record");

        pa.record_outcome(
            &sub_goals[1].goal_id,
            &SubGoalOutcome::Blocked {
                reason: "waiting".to_string(),
            },
        )
        .expect("record");

        let context = pa.build_plan_context(&root_id).expect("context exists");
        assert!(context.contains("[Plan: Build feature"));
        assert!(context.contains("[x] Design"));
        assert!(context.contains("[!] Implement"));
        assert!(context.contains("[ ] Test"));
        assert!(context.contains("[End plan]"));
    }

    #[test]
    fn test_build_plan_context_no_children() {
        let mut pa = PlanningAgent::new();

        let (root_id, _) = pa
            .create_plan("Simple task", None, &[])
            .expect("plan creation");

        let context = pa.build_plan_context(&root_id).expect("context");
        assert!(context.contains("[Current goal: Simple task"));
    }

    #[test]
    fn test_build_plan_context_missing_root() {
        let pa = PlanningAgent::new();
        assert!(pa.build_plan_context("nonexistent").is_none());
    }

    #[test]
    fn test_plan_execution_summary_context() {
        let summary = PlanExecutionSummary {
            root_goal_id: "root-1".to_string(),
            total_sub_goals: 3,
            completed: 2,
            blocked: 1,
            failed: 0,
            outputs: vec![
                (
                    "g-1".to_string(),
                    SubGoalOutcome::Completed {
                        output: "first done".to_string(),
                    },
                ),
                (
                    "g-2".to_string(),
                    SubGoalOutcome::Completed {
                        output: "second done".to_string(),
                    },
                ),
                (
                    "g-3".to_string(),
                    SubGoalOutcome::Blocked {
                        reason: "waiting".to_string(),
                    },
                ),
            ],
        };

        let ctx = summary.context_summary();
        assert!(ctx.contains("2/3 sub-goals completed"));
        assert!(ctx.contains("[done] g-1"));
        assert!(ctx.contains("[blocked] g-3"));
    }

    #[test]
    fn test_with_existing_tree() {
        use colony_focus::goal::Goal;
        use colony_focus::tree::GoalTree;

        let mut tree = GoalTree::new();
        tree.add_root(Goal::new("Existing").with_id("existing-root"))
            .expect("add root");

        let pa = PlanningAgent::with_tree(tree);
        assert!(pa.tree().get("existing-root").is_some());
    }
}
