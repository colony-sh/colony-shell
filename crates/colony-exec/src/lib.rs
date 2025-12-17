//! Command execution with fork/exec and job control
//!
//! This crate handles:
//! - Simple command execution (fork/exec)
//! - PATH resolution
//! - Environment passing
//! - Exit status capture
//! - Signal handling (future: job control)

mod builtins;
mod command;
mod command_sub;
mod error;
mod jobs;
mod path;
mod pipeline;
mod process;
mod redirect;

pub use builtins::{execute_builtin, is_builtin, BuiltinResult};
pub use command::execute_command;
pub use command_sub::execute_and_capture;
pub use error::{ExecutionError, Result};
pub use jobs::{init_job_control, Job, JobId, JobStatus, JobTable};
pub use path::resolve_command;
pub use pipeline::execute_pipeline;
pub use process::{ExitStatus, ProcessHandle};
pub use redirect::apply_redirects;

use colony_channels::ChannelManager;
use colony_expand::ExpansionContext;
use colony_parser::{Pipeline, SimpleCommand};
use colony_tuple::TupleSpace;

/// Main executor for shell commands
pub struct Executor {
    context: ExpansionContext,
    jobs: JobTable,
    /// Tuple space for reactive environment (Rules 42-47)
    tuple_space: TupleSpace,
    /// Extended channels (stdcontext, stdllm, stdtool, stdtrace)
    channels: ChannelManager,
}

impl Executor {
    pub fn new(context: ExpansionContext) -> Self {
        Self {
            context,
            jobs: JobTable::new(),
            tuple_space: TupleSpace::new(),
            channels: ChannelManager::new(),
        }
    }

    /// Get reference to channel manager
    pub fn channels(&self) -> &ChannelManager {
        &self.channels
    }

    /// Get mutable reference to channel manager
    pub fn channels_mut(&mut self) -> &mut ChannelManager {
        &mut self.channels
    }

    /// Get reference to job table
    pub fn jobs(&self) -> &JobTable {
        &self.jobs
    }

    /// Get reference to tuple space
    pub fn tuple_space(&self) -> &TupleSpace {
        &self.tuple_space
    }

    /// Execute a simple command
    ///
    /// Checks for builtins first, then falls back to external command execution.
    pub async fn execute_simple_command(&mut self, cmd: &SimpleCommand) -> Result<ExitStatus> {
        // Try builtin first (now with reactive tuple space!)
        tracing::debug!("Checking if command is builtin: {:?}", cmd.name);

        if let Some(builtin_result) = execute_builtin(cmd, &mut self.context, &self.jobs, &self.tuple_space, &mut self.channels) {
            tracing::debug!("Command is builtin, executing");
            let result = builtin_result?;

            // Handle exit builtin specially
            if result.is_exit() {
                // TODO: Propagate exit request to shell
                // For now, just return the exit code
            }

            return Ok(ExitStatus::new(result.exit_code()));
        }

        tracing::debug!("Not a builtin, executing as external command");
        // Not a builtin - execute as external command
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
