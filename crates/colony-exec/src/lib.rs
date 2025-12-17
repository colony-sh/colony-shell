//! Command execution with fork/exec and job control
//!
//! This crate handles:
//! - Simple command execution (fork/exec)
//! - PATH resolution
//! - Environment passing
//! - Exit status capture
//! - Signal handling (future: job control)

mod builtin_redirect;
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

    /// Execute a function with arguments
    ///
    /// Sets positional parameters, executes function body, handles return.
    pub async fn execute_function(
        &mut self,
        func_def: &colony_parser::FunctionDef,
        args: Vec<String>,
    ) -> Result<ExitStatus> {
        // Save current positional parameters
        let saved_params = self.context.get_all_positional_params().to_vec();

        // Set new positional parameters from function arguments
        self.context.set_positional_params(args);

        // Execute function body
        let mut last_exit = 0;
        for stmt in &func_def.body {
            // Execute each statement in the function body
            let exit_code = self.execute_statement_recursive(stmt).await?;
            last_exit = exit_code;

            // TODO: Check for ControlFlow::Return to exit early
        }

        // Restore positional parameters
        self.context.set_positional_params(saved_params);

        Ok(ExitStatus::new(last_exit))
    }

    /// Execute a simple command
    ///
    /// Checks for aliases, then builtins, then external commands.
    ///
    /// **F024 Phase 2:** Processes channel writes from child processes via tuple space.
    pub async fn execute_simple_command(&mut self, cmd: &SimpleCommand) -> Result<ExitStatus> {
        use colony_expand::{Expander, Variable};
        use colony_parser::AssignmentValue;

        // 0. Process assignments (arr=(a b c) or VAR=value)
        let mut expander = Expander::with_executor(self.context.clone(), |stmt, ctx| {
            crate::execute_and_capture(stmt, ctx)
        });

        for assignment in &cmd.env_assignments {
            match &assignment.value {
                AssignmentValue::Word(word) => {
                    // Expand the word
                    let fields = expander.expand_word(word)?;
                    let value = fields.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join(" ");
                    self.context.set_var(assignment.name.clone(), value);
                }
                AssignmentValue::Array(words) => {
                    // Expand each word and store as array
                    let mut array_values = Vec::new();
                    for word in words {
                        let fields = expander.expand_word(word)?;
                        for field in fields {
                            array_values.push(field.text.clone());
                        }
                    }
                    self.context.set_variable(assignment.name.clone(), Variable::Array(array_values));
                }
            }
        }

        // 1. Expand command name to check for aliases
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

        // 0c. Check for function call
        if let Some(func_def) = self.context.get_function(command_name).cloned() {
            // Expand arguments
            let mut args = Vec::new();
            for arg_word in &cmd.args {
                let fields = expander.expand_word(arg_word)?;
                for field in fields {
                    args.push(field.text);
                }
            }

            // Execute function with arguments as positional parameters
            return self.execute_function(&func_def, args).await;
        }

        // 1. Try builtin (now with reactive tuple space!)
        tracing::debug!("Checking if command is builtin: {:?}", cmd.name);

        if is_builtin(command_name) {
            tracing::debug!("Command is builtin, executing with redirect support");

            // Apply redirects for builtin (saves and restores FDs automatically)
            let _redirect_guard = builtin_redirect::BuiltinRedirectGuard::apply(&cmd.redirects, &mut self.context)?;

            // Execute builtin with redirects applied
            if let Some(builtin_result) = execute_builtin(cmd, &mut self.context, &self.jobs, &self.tuple_space, &mut self.channels) {
                let result = builtin_result?;

                // Handle exit builtin specially
                if result.is_exit() {
                    // TODO: Propagate exit request to shell
                    // For now, just return the exit code
                }

                // Guard drops here, restoring FDs automatically
                return Ok(ExitStatus::new(result.exit_code()));
            }
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

    /// Execute a statement recursively (for function bodies)
    ///
    /// Simplified executor that handles common statement types.
    /// Returns exit code.
    fn execute_statement_recursive<'a>(
        &'a mut self,
        stmt: &'a colony_parser::Statement,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i32>> + 'a>> {
        Box::pin(async move {
            use colony_parser::Statement;

            match stmt {
                Statement::SimpleCommand(cmd) => {
                    let status = Box::pin(self.execute_simple_command(cmd)).await?;
                    Ok(status.code())
                }
                Statement::Pipeline(pipeline) => {
                    let status = self.execute_pipeline(pipeline).await?;
                    Ok(status.code())
                }
                Statement::List(list) => {
                    // Execute first
                    let mut last_exit = self.execute_statement_recursive(&list.first).await?;

                    // Execute rest based on operators
                    for (op, stmt) in &list.rest {
                        use colony_parser::ListOp;
                        let should_exec = match op {
                            ListOp::Semi => true,
                            ListOp::And => last_exit == 0,
                            ListOp::Or => last_exit != 0,
                            ListOp::Background => false,
                        };

                        if should_exec {
                            last_exit = self.execute_statement_recursive(stmt).await?;
                        }
                    }

                    Ok(last_exit)
                }
                Statement::Return(_) => {
                    // Return statement - for now just return 0
                    // TODO: Implement proper return value and early exit
                    Ok(0)
                }
                Statement::Empty => {
                    // Empty statement - no-op
                    Ok(0)
                }
                _ => {
                    eprintln!("Statement type in function body not yet supported");
                    Ok(1)
                }
            }
        })
    }
}
