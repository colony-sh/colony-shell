//! Goal and task tracking system (F019)
//!
//! Persistent goal trees for agent workflows:
//! - `Goal` struct with hierarchical children, progress, blockers
//! - `GoalTree` stored in `~/.colony/goals/<project-hash>.json`
//! - Checkpoint creation (snapshot goal state at a point in time)
//! - Session summaries (what was accomplished)
//! - Resume capability (load previous goal state)

mod tree;
mod types;

pub use tree::GoalTree;
pub use types::{Checkpoint, Goal, GoalId, GoalStatus, SessionSummary};
