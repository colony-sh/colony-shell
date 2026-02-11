//! Goal trees, status tracking, and session continuity
//!
//! The focus system maintains a tree of goals with enforced state transitions,
//! SQLite persistence, and session restoration.
//!
//! ## Architecture
//!
//! - [`Goal`] — Individual goal with status, parent/child references
//! - [`GoalStatus`] — State machine: Active, Completed, Blocked, Cancelled
//! - [`GoalTree`] — In-memory tree with CRUD operations and traversals
//! - [`GoalStore`] — SQLite persistence with session support

pub mod error;
pub mod goal;
pub mod persistence;
pub mod tree;

pub use error::{FocusError, Result};
pub use goal::{Goal, GoalStatus};
pub use persistence::GoalStore;
pub use tree::GoalTree;
