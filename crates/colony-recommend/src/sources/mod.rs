//! Recommendation sources.

pub mod command;
pub mod git;
pub mod history;
pub mod path;

pub use command::CommandSource;
pub use git::GitSource;
pub use history::HistorySource;
pub use path::PathSource;
