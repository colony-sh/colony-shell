//! Command execution with fork/exec and job control
//!
//! This crate handles:
//! - Simple command execution (fork/exec)
//! - PATH resolution
//! - Environment passing
//! - Exit status capture
//! - Signal handling (future: job control)

mod command;
mod error;
mod path;
mod pipeline;
mod process;

pub use command::execute_command;
pub use error::{ExecutionError, Result};
pub use path::resolve_command;
pub use pipeline::execute_pipeline;
pub use process::{ExitStatus, ProcessHandle};

use colony_expand::ExpansionContext;
use colony_parser::{Pipeline, SimpleCommand};

/// Main executor for shell commands
pub struct Executor {
    context: ExpansionContext,
}

impl Executor {
    pub fn new(context: ExpansionContext) -> Self {
        Self { context }
    }

    /// Execute a simple command
    pub async fn execute_simple_command(&mut self, cmd: &SimpleCommand) -> Result<ExitStatus> {
        execute_command(cmd, &mut self.context).await
    }

    /// Execute a pipeline
    pub async fn execute_pipeline(&mut self, pipeline: &Pipeline) -> Result<ExitStatus> {
        execute_pipeline(pipeline, &mut self.context).await
    }

    /// Get mutable reference to expansion context
    pub fn context_mut(&mut self) -> &mut ExpansionContext {
        &mut self.context
    }

    /// Get reference to expansion context
    pub fn context(&self) -> &ExpansionContext {
        &self.context
    }
}
