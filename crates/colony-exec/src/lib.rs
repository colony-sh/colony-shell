//! Command execution with fork/exec and job control
//!
//! This crate handles:
//! - Simple command execution (fork/exec)
//! - PATH resolution
//! - Environment passing
//! - Exit status capture
//! - Signal handling (future: job control)

mod alias;
pub mod builtin_redirect;
mod builtins;
mod command;
mod command_sub;
mod error;
pub mod history;
mod jobs;
mod path;
mod pipeline;
mod process;
mod redirect;
mod xtrace;

pub use alias::{expand_alias, AliasExpansionResult};

pub use builtins::{execute_builtin, is_builtin, BuiltinResult};
pub use command::execute_command;
pub use command_sub::{execute_and_capture, execute_statement_for_process_sub};
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

    /// Consume the executor and return the context (for lastpipe variable persistence)
    pub fn into_context(self) -> ExpansionContext {
        self.context
    }

    /// Run EXIT trap if one is set
    /// 
    /// This is called when a subshell exits to ensure any EXIT trap set within
    /// the subshell is executed before the subshell completes.
    pub async fn run_exit_trap(&mut self) -> i32 {
        use colony_expand::TrapAction;
        use colony_expand::TrapSignal;
        use colony_parser::pest_api::parse;
        
        let exit_trap = self.context.get_trap(&TrapSignal::Exit).cloned();
        
        if let Some(TrapAction::Command(cmd)) = exit_trap {
            if let Ok(program) = parse(&cmd) {
                let mut last = 0;
                for stmt in &program.statements {
                    match self.execute_statement_recursive(stmt).await {
                        Ok(code) => {
                            last = code;
                            self.context.set_last_exit_status(code);
                        }
                        Err(_) => break,
                    }
                }
                return last;
            }
        }
        0
    }

    pub async fn execute_function(
        &mut self,
        func_def: &colony_parser::FunctionDef,
        args: Vec<String>,
    ) -> Result<ExitStatus> {
        let saved_params = self.context.get_all_positional_params().to_vec();
        self.context.set_positional_params(args);
        self.context.enter_function();

        let mut last_exit = 0;
        for stmt in &func_def.body {
            match self.execute_statement_recursive(stmt).await {
                Ok(code) => {
                    last_exit = code;
                    self.context.set_last_exit_status(code);
                }
                Err(ExecutionError::Return(code)) => {
                    last_exit = code;
                    self.context.set_last_exit_status(code);
                    break;
                }
                Err(e) => {
                    self.context.exit_function();
                    self.context.set_positional_params(saved_params);
                    return Err(e);
                }
            }
        }

        self.context.exit_function();
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

        // Check if this is a command that treats assignments as arguments
        // (alias, declare, export, local, readonly, typeset)
        let assignments_are_args = {
            // First word of command name (before expansion)
            let first_part = cmd.name.as_literal().unwrap_or("");
            matches!(first_part, "alias" | "declare" | "export" | "local" | "readonly" | "typeset")
        };

        // 0. Process assignments (arr=(a b c) or VAR=value)
        // BUT if this is an assignment-accepting builtin, skip processing and pass as args
        let mut expander = Expander::with_executor(self.context.clone(), |stmt, ctx| {
            crate::execute_and_capture(stmt, ctx)
        });

        // POSIX: Detect if this is an assignment-only command (no actual command to execute)
        // In this case, assignments should be applied to the shell context.
        // For real commands with prefix assignments (like "IFS=: echo $var"),
        // the assignments should ONLY be passed to the child process environment,
        // NOT applied to the shell context before expansion.
        let is_assignment_only = cmd.name.as_literal() == Some(":")
            && cmd.args.is_empty()
            && !cmd.env_assignments.is_empty();

        // Convert env_assignments to argument words for builtins that expect them
        let cmd = if assignments_are_args && !cmd.env_assignments.is_empty() {
            let mut new_cmd = cmd.clone();
            let mut extra_args = Vec::new();
            
            for assignment in &cmd.env_assignments {
                // Reconstruct the assignment as a string argument
                let mut arg_str = assignment.name.clone();
                if let Some(ref subscript) = assignment.subscript {
                    match subscript {
                        colony_parser::ArraySubscript::Index(expr) => {
                            arg_str.push('[');
                            // Simple handling for common cases
                            match &**expr {
                                colony_parser::Expression::Int(n, _) => arg_str.push_str(&n.to_string()),
                                colony_parser::Expression::Str(s, _) => arg_str.push_str(s),
                                _ => {}
                            }
                            arg_str.push(']');
                        }
                        colony_parser::ArraySubscript::All => arg_str.push_str("[@]"),
                        colony_parser::ArraySubscript::Star => arg_str.push_str("[*]"),
                    }
                }
                arg_str.push_str(if assignment.op == colony_parser::AssignOp::Append { "+=" } else { "=" });
                
                match &assignment.value {
                    AssignmentValue::Word(word) => {
                        if let Some(lit) = word.as_literal() {
                            arg_str.push_str(lit);
                        } else {
                            // Expand the word to get the value
                            if let Ok(fields) = expander.expand_word(word) {
                                arg_str.push_str(&fields.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join(" "));
                            }
                        }
                    }
                    AssignmentValue::Array(words) => {
                        arg_str.push('(');
                        let mut word_strs = Vec::new();
                        for w in words {
                            if let Some(lit) = w.as_literal() {
                                word_strs.push(lit.to_string());
                            }
                        }
                        arg_str.push_str(&word_strs.join(" "));
                        arg_str.push(')');
                    }
                    AssignmentValue::AssocArray(entries) => {
                        arg_str.push('(');
                        let entry_strs: Vec<String> = entries.iter()
                            .map(|(k, v)| format!("[{}]={}", k, v))
                            .collect();
                        arg_str.push_str(&entry_strs.join(" "));
                        arg_str.push(')');
                    }
                }
                
                extra_args.push(colony_parser::Word::literal(&arg_str, assignment.span.clone()));
            }
            
            // Prepend the assignment args to the existing args
            extra_args.extend(new_cmd.args);
            new_cmd.args = extra_args;
            new_cmd.env_assignments = vec![]; // Clear - we've converted them to args
            std::borrow::Cow::Owned(new_cmd)
        } else {
            std::borrow::Cow::Borrowed(cmd)
        };
        let cmd = cmd.as_ref();

        // Only apply assignments to shell context for assignment-only commands.
        // For real commands with prefix assignments, we skip this and pass them to child env.
        if is_assignment_only {
            for assignment in &cmd.env_assignments {
                if let Some(ref subscript) = assignment.subscript {
                    match &assignment.value {
                        AssignmentValue::Word(word) => {
                            let value = expander.expand_word_for_assignment(word)?;
                            
                            let key = match subscript {
                                colony_parser::ArraySubscript::Index(expr) => {
                                    match &**expr {
                                        colony_parser::Expression::Variable { name, .. } => {
                                            if let Some(Variable::AssocArray(_)) = self.context.get_variable(&assignment.name) {
                                                name.clone()
                                            } else {
                                                self.context.get_var_or_env(name).unwrap_or_else(|| name.clone())
                                            }
                                        }
                                        colony_parser::Expression::Str(s, _) => {
                                            self.expand_subscript_string(s)
                                        }
                                        colony_parser::Expression::Int(n, _) => n.to_string(),
                                        _ => String::new()
                                    }
                                }
                                _ => String::new()
                            };
                            
                            if let Some(Variable::AssocArray(existing)) = self.context.get_variable(&assignment.name) {
                                let mut map = existing.clone();
                                map.insert(key, value);
                                self.context.set_variable(assignment.name.clone(), Variable::AssocArray(map));
                            } else {
                                let arr = match self.context.get_variable(&assignment.name) {
                                    Some(Variable::Array(existing)) => existing.clone(),
                                    _ => Vec::new(),
                                };
                                
                                let index = key.parse::<usize>().unwrap_or(0);
                                let mut new_arr = arr;
                                if index >= new_arr.len() {
                                    new_arr.resize(index + 1, String::new());
                                }
                                new_arr[index] = value;
                                self.context.set_variable(assignment.name.clone(), Variable::Array(new_arr));
                            }
                        }
                        AssignmentValue::Array(_) => {
                            eprintln!("Warning: Cannot assign array to array element");
                        }
                        AssignmentValue::AssocArray(_) => {
                            eprintln!("Warning: Cannot assign assoc array to array element");
                        }
                    }
                } else {
                    match &assignment.value {
                        AssignmentValue::Word(word) => {
                            let value = expander.expand_word_for_assignment(word)?;
                            xtrace::trace_assignment(&self.context, &assignment.name, &value);
                            if assignment.op == colony_parser::AssignOp::Append {
                                if self.context.is_integer(&assignment.name) {
                                    let existing = self.context.get_var_or_env(&assignment.name)
                                        .and_then(|s| s.parse::<i64>().ok())
                                        .unwrap_or(0);
                                    let addend = value.parse::<i64>().unwrap_or(0);
                                    let result = (existing + addend).to_string();
                                    if !self.context.set_var(assignment.name.clone(), result) {
                                        eprintln!("colony: {}: readonly variable", &assignment.name);
                                        return Ok(ExitStatus::new(1));
                                    }
                                } else {
                                    let existing = self.context.get_var_or_env(&assignment.name).unwrap_or_default();
                                    if !self.context.set_var(assignment.name.clone(), existing + &value) {
                                        eprintln!("colony: {}: readonly variable", &assignment.name);
                                        return Ok(ExitStatus::new(1));
                                    }
                                }
                            } else {
                                if !self.context.set_var(assignment.name.clone(), value) {
                                    eprintln!("colony: {}: readonly variable", &assignment.name);
                                    return Ok(ExitStatus::new(1));
                                }
                            }
                        }
                        AssignmentValue::Array(words) => {
                            let mut new_values = Vec::new();
                            for word in words {
                                let fields = expander.expand_word(word)?;
                                for field in fields {
                                    new_values.push(field.text.clone());
                                }
                            }
                            if assignment.op == colony_parser::AssignOp::Append {
                                let mut existing = match self.context.get_variable(&assignment.name) {
                                    Some(Variable::Array(arr)) => arr.clone(),
                                    Some(Variable::String(s)) => vec![s.clone()],
                                    Some(Variable::AssocArray(_)) => Vec::new(),
                                    Some(Variable::NameRef(_)) => Vec::new(),
                                    None => Vec::new(),
                                };
                                existing.extend(new_values);
                                self.context.set_variable(assignment.name.clone(), Variable::Array(existing));
                            } else {
                                self.context.set_variable(assignment.name.clone(), Variable::Array(new_values));
                            }
                        }
                        AssignmentValue::AssocArray(entries) => {
                            use std::collections::HashMap;
                            let mut map = HashMap::new();
                            for (key, val) in entries {
                                map.insert(key.clone(), val.clone());
                            }
                            self.context.set_variable(assignment.name.clone(), Variable::AssocArray(map));
                        }
                    }
                }
            }
        }

        // For assignment-only commands, we've already applied changes to self.context directly.
        // Don't sync from expander's stale copy. But DO sync the exit status from command substitutions.
        if !is_assignment_only {
            self.context.sync_variables_from(expander.context());
        } else {
            // POSIX: For bare assignments (no command), the exit status is from the last
            // command substitution, or 0 if there were none. Return immediately - don't
            // execute the synthetic ":" command which would overwrite the exit status.
            let exit_status = expander.context().last_exit_status();
            self.context.set_last_exit_status(exit_status);
            return Ok(ExitStatus::new(exit_status));
        }

        let name_fields = expander.expand_word(&cmd.name)?;
        if name_fields.is_empty() {
            if cmd.args.is_empty() {
                return Ok(ExitStatus::new(self.context.last_exit_status()));
            }
            return Err(ExecutionError::CommandNotFound("<empty>".to_string()));
        }
        let command_name = &name_fields[0].text;
        
        // When "$@" expands to multiple fields in command position,
        // extra fields become arguments (e.g., "$@" with args [echo, hello] -> run echo with arg hello)
        let extra_args_from_name: Vec<String> = name_fields[1..].iter().map(|f| f.text.clone()).collect();

        if let Some(func_def) = self.context.get_function(command_name).cloned() {
            let mut args = extra_args_from_name;
            for arg_word in &cmd.args {
                let fields = expander.expand_word(arg_word)?;
                for field in fields {
                    args.push(field.text);
                }
            }

            self.context.sync_variables_from(expander.context());
            xtrace::trace_simple_command(&self.context, command_name, &args);
            let _redirect_guard = builtin_redirect::BuiltinRedirectGuard::apply(&cmd.redirects, &mut self.context)?;
            return self.execute_function(&func_def, args).await;
        }

        // Alias expansion with cycle detection and re-parsing support
        let mut expanding_aliases = std::collections::HashSet::new();
        match alias::expand_alias(command_name, &self.context, &expanding_aliases) {
            alias::AliasExpansionResult::Reparsed(statements) => {
                let mut last_exit = 0;
                for (i, stmt) in statements.iter().enumerate() {
                    if i == statements.len() - 1 {
                        if let colony_parser::Statement::SimpleCommand(mut last_cmd) = stmt.clone() {
                            last_cmd.args.extend(cmd.args.clone());
                            last_cmd.redirects.extend(cmd.redirects.clone());
                            last_exit = self.execute_statement_recursive(&colony_parser::Statement::SimpleCommand(last_cmd)).await?;
                        } else {
                            last_exit = self.execute_statement_recursive(stmt).await?;
                        }
                    } else {
                        last_exit = self.execute_statement_recursive(stmt).await?;
                    }
                    self.context.set_last_exit_status(last_exit);
                }
                return Ok(ExitStatus::new(last_exit));
            }
            alias::AliasExpansionResult::SimpleExpansion { new_command, alias_args, expand_next } => {
                expanding_aliases.insert(command_name.to_string());
                
                let mut new_cmd = cmd.clone();
                new_cmd.name = colony_parser::Word::literal(&new_command, cmd.span.clone());
                
                let mut new_args = Vec::new();
                for alias_arg in &alias_args {
                    new_args.push(colony_parser::Word::literal(alias_arg, cmd.span.clone()));
                }
                
                // Handle trailing space expansion (e.g., alias sudo="sudo ")
                let mut skip_first_arg = false;
                if expand_next && !cmd.args.is_empty() {
                    if let Some(first_arg) = cmd.args.first() {
                        if let Some(arg_name) = first_arg.as_literal() {
                            if !expanding_aliases.contains(arg_name) {
                                if let alias::AliasExpansionResult::SimpleExpansion { 
                                    new_command: arg_cmd, 
                                    alias_args: arg_alias_args, 
                                    .. 
                                } = alias::expand_alias(arg_name, &self.context, &expanding_aliases) {
                                    new_args.push(colony_parser::Word::literal(&arg_cmd, first_arg.span.clone()));
                                    for arg in &arg_alias_args {
                                        new_args.push(colony_parser::Word::literal(arg, first_arg.span.clone()));
                                    }
                                    skip_first_arg = true;
                                }
                            }
                        }
                    }
                }
                
                let args_to_add = if skip_first_arg { &cmd.args[1..] } else { &cmd.args[..] };
                new_args.extend(args_to_add.iter().cloned());
                new_cmd.args = new_args;
                
                // Recursively execute but avoid infinite loop by checking if command changed
                if new_command != *command_name {
                    return Box::pin(self.execute_simple_command(&new_cmd)).await;
                }
                // If command name didn't change (alias ls=ls), fall through to normal execution
                // with the new_cmd that has alias_args prepended
                return Box::pin(async move {
                    // Execute with the expanded command (avoiding alias lookup)
                    let status = self.execute_external_command_with_channels(&new_cmd).await?;
                    self.sync_channels_from_tuple_space();
                    Ok(status)
                }).await;
            }
            alias::AliasExpansionResult::CycleDetected | alias::AliasExpansionResult::NoAlias => {
                // No alias or cycle detected - continue with normal execution
            }
        }

        // 1. Try builtin (now with reactive tuple space!)
        tracing::debug!("Checking if command is builtin: {:?}", cmd.name);

        if is_builtin(command_name) {
            tracing::debug!("Command is builtin, executing with redirect support");
            
            if !is_assignment_only {
                self.context.sync_variables_from(expander.context());
            }
            
            let is_assignment_only_noop = command_name == ":" 
                && cmd.args.is_empty()
                && !cmd.env_assignments.is_empty();
            if !is_assignment_only_noop {
                let mut trace_args = extra_args_from_name.clone();
                for arg_word in &cmd.args {
                    if let Ok(fields) = expander.expand_word(arg_word) {
                        for field in fields {
                            trace_args.push(field.text);
                        }
                    }
                }
                xtrace::trace_simple_command(&self.context, command_name, &trace_args);
            }

            let _redirect_guard = if command_name == "exec" {
                None
            } else {
                Some(builtin_redirect::BuiltinRedirectGuard::apply(&cmd.redirects, &mut self.context)?)
            };

            if let Some(builtin_result) = execute_builtin(cmd, &mut self.context, &self.jobs, &self.tuple_space, &mut self.channels) {
                let result = builtin_result?;

                if result.is_exit() {
                    return Err(ExecutionError::Exit(result.exit_code()));
                }
                
                if let BuiltinResult::Source(file_path, source_args) = result {
                    return self.execute_source_file(&file_path, source_args).await;
                }
                
                if let BuiltinResult::Eval(eval_string) = result {
                    return self.execute_eval_string(&eval_string).await;
                }

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
            let extra_args_from_name: Vec<String> = name_fields[1..].iter().map(|f| f.text.clone()).collect();

            // Expand arguments
            let mut args = vec![command_name.clone()];
            args.extend(extra_args_from_name);
            for arg_word in &cmd_clone.args {
                let fields = expander.expand_word(arg_word)?;
                for field in fields {
                    args.push(field.text);
                }
            }

            xtrace::trace_simple_command(&context_clone, command_name, &args[1..]);

            // Resolve command path
            let path_env = context_clone.get_env("PATH");
            let program_path = resolve_command(command_name, path_env)?;

            let mut env: Vec<(String, String)> = std::env::vars().collect();
            
            for assignment in &cmd_clone.env_assignments {
                if assignment.subscript.is_none() {
                    if let colony_parser::AssignmentValue::Word(word) = &assignment.value {
                        let value = expander.expand_word_for_assignment(word)?;
                        env.retain(|(k, _)| k != &assignment.name);
                        env.push((assignment.name.clone(), value));
                    }
                }
            }

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
    
    pub async fn execute_source_file(&mut self, file_path: &std::path::Path, args: Vec<String>) -> Result<ExitStatus> {
        use colony_parser::pest_api::parse;
        
        let content = std::fs::read_to_string(file_path).map_err(|e| {
            ExecutionError::IoError(std::io::Error::new(
                e.kind(),
                format!("source: {}: {}", file_path.display(), e),
            ))
        })?;
        
        let program = parse(&content).map_err(|e| {
            ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("source: {}: parse error: {}", file_path.display(), e),
            ))
        })?;
        
        let saved_positional = self.context.get_all_positional_params().to_vec();
        if !args.is_empty() {
            self.context.set_positional_params(args);
        }
        
        let mut last_exit = 0;
        for stmt in &program.statements {
            match self.execute_statement_recursive(stmt).await {
                Ok(code) => {
                    last_exit = code;
                    self.context.set_last_exit_status(code);
                }
                Err(ExecutionError::Return(code)) => {
                    last_exit = code;
                    self.context.set_last_exit_status(code);
                    break;
                }
                Err(ExecutionError::Exit(code)) => {
                    self.context.set_positional_params(saved_positional);
                    return Err(ExecutionError::Exit(code));
                }
                Err(e) => {
                    self.context.set_positional_params(saved_positional);
                    return Err(e);
                }
            }
        }
        
        self.context.set_positional_params(saved_positional);
        Ok(ExitStatus::new(last_exit))
    }
    
    pub async fn execute_eval_string(&mut self, eval_string: &str) -> Result<ExitStatus> {
        use colony_parser::pest_api::parse;
        
        let program = parse(eval_string).map_err(|e| {
            ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("eval: parse error: {}", e),
            ))
        })?;
        
        let mut last_exit = 0;
        for stmt in &program.statements {
            match self.execute_statement_recursive(stmt).await {
                Ok(code) => {
                    last_exit = code;
                    self.context.set_last_exit_status(code);
                }
                Err(ExecutionError::Return(code)) => {
                    last_exit = code;
                    self.context.set_last_exit_status(code);
                    break;
                }
                Err(ExecutionError::Exit(code)) => {
                    return Err(ExecutionError::Exit(code));
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        
        Ok(ExitStatus::new(last_exit))
    }

    /// Get mutable reference to expansion context
    pub fn context_mut(&mut self) -> &mut ExpansionContext {
        &mut self.context
    }

    /// Get reference to expansion context
    pub fn context(&self) -> &ExpansionContext {
        &self.context
    }
    
    fn check_errexit(&self, code: i32) -> Result<i32> {
        if self.context.shell_options().errexit && code != 0 {
            Err(ExecutionError::Errexit(code))
        } else {
            Ok(code)
        }
    }

    fn expand_subscript_string(&self, s: &str) -> String {
        if s.starts_with('$') {
            let var_name = s.trim_start_matches('$');
            self.context.get_var_or_env(var_name).unwrap_or_else(|| s.to_string())
        } else {
            s.to_string()
        }
    }

    /// Execute a statement recursively (for function bodies)
    ///
    /// Simplified executor that handles common statement types.
    /// Returns exit code.
    pub fn execute_statement_recursive<'a>(
        &'a mut self,
        stmt: &'a colony_parser::Statement,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<i32>> + 'a>> {
        Box::pin(async move {
            use colony_parser::Statement;

            match stmt {
                Statement::SimpleCommand(cmd) => {
                    let status = Box::pin(self.execute_simple_command(cmd)).await?;
                    let code = status.code();
                    self.context.set_last_exit_status(code);
                    self.check_errexit(code)
                }
                Statement::Pipeline(pipeline) => {
                    if pipeline.elements.len() == 1 && !pipeline.elements[0].negated {
                        return self.execute_statement_recursive(&pipeline.elements[0].command).await;
                    }
                    let status = self.execute_pipeline(pipeline).await?;
                    let code = status.code();
                    self.context.set_last_exit_status(code);
                    Ok(code)
                }
                Statement::List(list) => {
                    // Execute first
                    let mut last_exit = self.execute_statement_recursive(&list.first).await?;
                    self.context.set_last_exit_status(last_exit);

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
                            self.context.set_last_exit_status(last_exit);
                        }
                    }

                    Ok(last_exit)
                }
                Statement::Return(ret_stmt) => {
                    let code = if let Some(ref expr) = ret_stmt.value {
                        use colony_parser::Expression;
                        match expr {
                            Expression::Int(n, _) => *n as i32,
                            _ => 0
                        }
                    } else {
                        self.context.last_exit_status()
                    };
                    Err(ExecutionError::Return(code))
                }
            Statement::Empty => {
                // Empty statement preserves previous exit status
                Ok(self.context.last_exit_status())
            }
                Statement::For(for_stmt) => {
                    self.execute_for_loop_internal(for_stmt).await
                }
                Statement::If(if_stmt) => {
                    self.execute_if_internal(if_stmt).await
                }
                Statement::While(while_stmt) => {
                    self.execute_while_internal(while_stmt).await
                }
                Statement::Until(until_stmt) => {
                    self.execute_until_internal(until_stmt).await
                }
                Statement::Case(case_stmt) => {
                    self.execute_case_internal(case_stmt).await
                }
                Statement::BraceGroup(brace) => {
                    let mut last = 0;
                    for stmt in &brace.body {
                        last = self.execute_statement_recursive(stmt).await?;
                    }
                    Ok(last)
                }
                Statement::Subshell(subshell) => {
                    let mut subshell_context = self.context.clone();
                    subshell_context.clear_non_ignored_traps();
                    let mut subshell_executor = Executor::new(subshell_context);
                    let mut last = 0;
                    for stmt in &subshell.body {
                        last = subshell_executor.execute_statement_recursive(stmt).await?;
                    }
                    subshell_executor.run_exit_trap().await;
                    Ok(last)
                }
                Statement::FunctionDef(func_def) => {
                    self.context.define_function(func_def.name.clone(), func_def.clone());
                    Ok(0)
                }
                Statement::Arithmetic(arith) => {
                    match colony_expand::expand_arithmetic(&arith.expr, &mut self.context) {
                        Ok(result) => {
                            let value: i64 = result.parse().unwrap_or(0);
                            let code = if value == 0 { 1 } else { 0 };
                            Ok(code)
                        }
                        Err(e) => {
                            eprintln!("colony: arithmetic error: {}", e);
                            Ok(1)
                        }
                    }
                }
                other => {
                    eprintln!("Statement type not yet supported in function: {:?}", other);
                    Ok(1)
                }
            }
        })
    }
    
    async fn execute_for_loop_internal(&mut self, for_stmt: &colony_parser::ForStatement) -> Result<i32> {
        use colony_expand::Expander;
        
        let _redirect_guard = builtin_redirect::BuiltinRedirectGuard::apply(&for_stmt.redirects, &mut self.context)?;
        
        let mut expander = Expander::with_executor(self.context.clone(), |stmt, ctx| {
            crate::execute_and_capture(stmt, ctx)
        });
        
        let mut words_expanded = Vec::new();
        for word in &for_stmt.words {
            let fields = expander.expand_word(word)?;
            for field in fields {
                words_expanded.push(field.text);
            }
        }
        
        let mut last_exit = 0;
        for value in words_expanded {
            self.context.set_var(for_stmt.var.clone(), value);
            
            for stmt in &for_stmt.body {
                last_exit = self.execute_statement_recursive(stmt).await?;
            }
        }
        
        Ok(last_exit)
    }
    
    async fn execute_if_internal(&mut self, if_stmt: &colony_parser::IfStatement) -> Result<i32> {
        let cond_exit = self.execute_condition_internal(&if_stmt.condition).await?;
        
        if cond_exit == 0 {
            let mut last = 0;
            for stmt in &if_stmt.then_body {
                last = self.execute_statement_recursive(stmt).await?;
            }
            return Ok(last);
        }
        
        for elif in &if_stmt.elif_branches {
            let cond_exit = self.execute_condition_internal(&elif.condition).await?;
            if cond_exit == 0 {
                let mut last = 0;
                for stmt in &elif.body {
                    last = self.execute_statement_recursive(stmt).await?;
                }
                return Ok(last);
            }
        }
        
        if let Some(ref else_body) = if_stmt.else_body {
            let mut last = 0;
            for stmt in else_body {
                last = self.execute_statement_recursive(stmt).await?;
            }
            return Ok(last);
        }
        
        Ok(0)
    }
    
    async fn execute_condition_internal(&mut self, condition: &colony_parser::Condition) -> Result<i32> {
        let saved_errexit = self.context.shell_options().errexit;
        self.context.shell_options_mut().errexit = false;
        
        let result = match condition {
            colony_parser::Condition::Command(cmd) => {
                self.execute_statement_recursive(cmd).await
            }
            colony_parser::Condition::Expression(_) => {
                Ok(1)
            }
        };
        
        self.context.shell_options_mut().errexit = saved_errexit;
        result
    }
    
    async fn execute_while_internal(&mut self, while_stmt: &colony_parser::WhileStatement) -> Result<i32> {
        let _redirect_guard = builtin_redirect::BuiltinRedirectGuard::apply(&while_stmt.redirects, &mut self.context)?;
        
        let mut last_exit = 0;
        loop {
            let cond_exit = self.execute_condition_internal(&while_stmt.condition).await?;
            if cond_exit != 0 {
                break;
            }
            for stmt in &while_stmt.body {
                last_exit = self.execute_statement_recursive(stmt).await?;
            }
        }
        Ok(last_exit)
    }
    
    async fn execute_until_internal(&mut self, until_stmt: &colony_parser::UntilStatement) -> Result<i32> {
        let _redirect_guard = builtin_redirect::BuiltinRedirectGuard::apply(&until_stmt.redirects, &mut self.context)?;
        
        let mut last_exit = 0;
        loop {
            let cond_exit = self.execute_condition_internal(&until_stmt.condition).await?;
            if cond_exit == 0 {
                break;
            }
            for stmt in &until_stmt.body {
                last_exit = self.execute_statement_recursive(stmt).await?;
            }
        }
        Ok(last_exit)
    }

    async fn execute_case_internal(&mut self, case_stmt: &colony_parser::CaseStatement) -> Result<i32> {
        use colony_expand::Expander;
        
        let mut expander = Expander::with_executor(self.context.clone(), |stmt, ctx| {
            crate::execute_and_capture(stmt, ctx)
        });

        let value_fields = expander.expand_word(&case_stmt.value)?;
        let value_str = if value_fields.is_empty() {
            String::new()
        } else {
            value_fields[0].text.clone()
        };

        let mut last_exit = 0;
        let mut execute_next = false;
        let mut found_first_match = false;

        for arm in &case_stmt.arms {
            let mut should_execute = false;

            if execute_next {
                should_execute = true;
                execute_next = false;
            } else if !found_first_match {
                for pattern_word in &arm.patterns {
                    let pattern_fields = expander.expand_word_no_glob(pattern_word)?;
                    let pattern = if pattern_fields.is_empty() {
                        ""
                    } else {
                        &pattern_fields[0].text
                    };
                    
                    if Self::pattern_matches(&value_str, pattern) {
                        should_execute = true;
                        break;
                    }
                }
            }

            if should_execute {
                for stmt in &arm.body {
                    match self.execute_statement_recursive(stmt).await {
                        Ok(code) => {
                            last_exit = code;
                            self.context.set_last_exit_status(code);
                        }
                        Err(ExecutionError::Break) => {
                            return Ok(last_exit);
                        }
                        Err(ExecutionError::Continue) => {
                            return Ok(last_exit);
                        }
                        Err(e) => return Err(e),
                    }
                }

                match arm.terminator {
                    colony_parser::CaseTerminator::Break => {
                        break;
                    }
                    colony_parser::CaseTerminator::Fallthrough => {
                        execute_next = true;
                        found_first_match = true;
                    }
                    colony_parser::CaseTerminator::Continue => {
                    }
                }
            }
        }

        Ok(last_exit)
    }

    fn pattern_matches(value: &str, pattern: &str) -> bool {
        use colony_expand::pathname::match_extglob_pattern;
        
        if colony_expand::pathname::contains_extglob(pattern) {
            return match_extglob_pattern(pattern, value);
        }
        
        use glob::Pattern;
        
        match Pattern::new(pattern) {
            Ok(glob_pattern) => glob_pattern.matches(value),
            Err(_) => value == pattern,
        }
    }
}
