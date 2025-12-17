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
pub use redirect::{
    apply_redirects, apply_channel_redirects_in_child, prepare_channel_redirects,
    process_channel_writes, ChannelPipe, is_channel_fd,
    STDCONTEXT, STDLLM, STDTOOL, STDTRACE,
};

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
    /// Checks for aliases, then builtins, then external commands.
    ///
    /// **F024 Phase 2:** Processes channel writes from child processes via tuple space.
    pub async fn execute_simple_command(&mut self, cmd: &SimpleCommand) -> Result<ExitStatus> {
        use colony_expand::Expander;

        // 0. Expand command name to check for aliases
        let mut expander = Expander::with_executor(self.context.clone(), |stmt, ctx| {
            crate::execute_and_capture(stmt, ctx)
        });

        let name_fields = expander.expand_word(&cmd.name)?;
        if name_fields.is_empty() {
            return Err(ExecutionError::CommandNotFound("<empty>".to_string()));
        }
        let command_name = &name_fields[0].text;

        // 0b. Check for alias expansion
        if let Some(alias_value) = self.context.get_alias(command_name) {
            // Parse alias value and create new command
            // Simple approach: split on whitespace and prepend to args
            let alias_words: Vec<String> = alias_value.split_whitespace()
                .map(|s| s.to_string())
                .collect();

            if !alias_words.is_empty() {
                // Create new command with alias expanded
                let mut new_cmd = cmd.clone();
                new_cmd.name = colony_parser::Word::literal(&alias_words[0], cmd.span.clone());

                // Prepend alias args to original args
                let mut new_args = vec![];
                for alias_arg in &alias_words[1..] {
                    new_args.push(colony_parser::Word::literal(alias_arg, cmd.span.clone()));
                }
                new_args.extend(cmd.args.clone());
                new_cmd.args = new_args;

                // Recursively execute the expanded command
                // Box the future to avoid recursion size issues
                return Box::pin(self.execute_simple_command(&new_cmd)).await;
            }
        }

        // 1. Try builtin (now with reactive tuple space!)
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

        // 2. Execute external command with channel redirect support
        let status = self.execute_external_command_with_channels(cmd).await?;

        // F024 Phase 2: Sync channel writes from tuple space to channels
        self.sync_channels_from_tuple_space();

        Ok(status)
    }

    /// Execute external command and process channel writes
    async fn execute_external_command_with_channels(&mut self, cmd: &SimpleCommand) -> Result<ExitStatus> {
        // Execute command (this returns ProcessHandle with channel_pipes)
        let cmd_clone = cmd.clone();
        let mut context_clone = self.context.clone();
        let tuple_space_clone = self.tuple_space.clone();

        tokio::task::spawn_blocking(move || {
            // Execute command using existing execute_command_blocking from command.rs
            // But we need to intercept the ProcessHandle...
            // Actually, let me inline the logic here to capture channel_pipes

            use crate::path::resolve_command;
            use crate::process::spawn_process;
            use colony_expand::Expander;

            // Create expander with command substitution support
            let mut expander = Expander::with_executor(context_clone.clone(), |stmt, ctx| {
                crate::execute_and_capture(stmt, ctx)
            });

            // Expand command name
            let name_fields = expander.expand_word(&cmd_clone.name)?;
            if name_fields.is_empty() {
                return Err(ExecutionError::CommandNotFound("<empty>".to_string()));
            }
            let command_name = &name_fields[0].text;

            // Expand arguments
            let mut args = vec![command_name.clone()];
            for arg_word in &cmd_clone.args {
                let fields = expander.expand_word(arg_word)?;
                for field in fields {
                    args.push(field.text);
                }
            }

            // Resolve command path
            let path_env = context_clone.get_env("PATH");
            let program_path = resolve_command(command_name, path_env)?;

            // Build environment
            let env: Vec<(String, String)> = std::env::vars().collect();

            // Spawn process (F024: now returns channel_pipes)
            let mut handle = spawn_process(&program_path, &args, &env, &cmd_clone.redirects, &mut context_clone)?;

            // Extract channel pipes before wait consumes handle
            let channel_pipes = handle.take_channel_pipes();

            // Wait for child
            let status = handle.wait()?;

            // F024 Phase 2: Process channel writes if any
            if !channel_pipes.is_empty() {
                process_channel_writes(channel_pipes, &tuple_space_clone)?;
            }

            Ok(status)
        })
        .await
        .map_err(|e| ExecutionError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e)))?
    }

    /// Sync channel writes from tuple space to channels
    ///
    /// Polls tuple space for ("channel_write", fd, content, timestamp) tuples
    /// and flushes them to the appropriate channels.
    fn sync_channels_from_tuple_space(&mut self) {
        use colony_tuple::{Tuple, Value};

        tracing::debug!("sync_channels_from_tuple_space: polling for channel_write tuples");

        // Poll for all channel_write tuples
        let pattern = Tuple::pattern(&[
            Value::Str("channel_write".to_string()),
            Value::Wildcard, // fd
            Value::Wildcard, // content
            Value::Wildcard, // timestamp
        ]);

        let mut count = 0;
        while let Some(tuple) = self.tuple_space.inp(pattern.clone()) {
            count += 1;
            tracing::debug!("Found channel_write tuple #{}", count);

            if let (Some(Value::Int(fd)), Some(Value::Str(content))) =
                (tuple.fields.get(1), tuple.fields.get(2))
            {
                tracing::debug!("Syncing: fd={}, content_len={}", fd, content.len());

                // Write to appropriate channel
                if let Some(channel) = self.channels.get_mut(*fd as colony_channels::ChannelId) {
                    use colony_channels::Channel;
                    match channel.write(content.as_bytes()) {
                        Ok(n) => tracing::debug!("Wrote {} bytes to channel {}", n, fd),
                        Err(e) => tracing::warn!("Failed to write to channel {}: {}", fd, e),
                    }
                } else {
                    tracing::warn!("Channel {} not found", fd);
                }
            } else {
                tracing::warn!("Invalid channel_write tuple format");
            }
        }

        tracing::debug!("sync_channels_from_tuple_space: processed {} tuples", count);
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
