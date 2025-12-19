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

    pub async fn execute_function(
        &mut self,
        func_def: &colony_parser::FunctionDef,
        args: Vec<String>,
    ) -> Result<ExitStatus> {
        let saved_params = self.context.get_all_positional_params().to_vec();
        self.context.set_positional_params(args);

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
                    self.context.set_positional_params(saved_params);
                    return Err(e);
                }
            }
        }

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

        for assignment in &cmd.env_assignments {
            if let Some(ref subscript) = assignment.subscript {
                match &assignment.value {
                    AssignmentValue::Word(word) => {
                        let fields = expander.expand_word(word)?;
                        let value = fields.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join(" ");
                        
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
                                    colony_parser::Expression::Str(s, _) => s.clone(),
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
                        let fields = expander.expand_word(word)?;
                        let value = fields.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join(" ");
                        if assignment.op == colony_parser::AssignOp::Append {
                            let existing = self.context.get_var_or_env(&assignment.name).unwrap_or_default();
                            self.context.set_var(assignment.name.clone(), existing + &value);
                        } else {
                            self.context.set_var(assignment.name.clone(), value);
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

        let name_fields = expander.expand_word(&cmd.name)?;
        if name_fields.is_empty() {
            return Err(ExecutionError::CommandNotFound("<empty>".to_string()));
        }
        let command_name = &name_fields[0].text;

        if let Some(func_def) = self.context.get_function(command_name).cloned() {
            let mut args = Vec::new();
            for arg_word in &cmd.args {
                let fields = expander.expand_word(arg_word)?;
                for field in fields {
                    args.push(field.text);
                }
            }

            let _redirect_guard = builtin_redirect::BuiltinRedirectGuard::apply(&cmd.redirects, &mut self.context)?;
            return self.execute_function(&func_def, args).await;
        }

        if let Some(alias_value) = self.context.get_alias(command_name) {
            let alias_words: Vec<String> = alias_value.split_whitespace()
                .map(|s| s.to_string())
                .collect();

            if !alias_words.is_empty() {
                let mut new_cmd = cmd.clone();
                new_cmd.name = colony_parser::Word::literal(&alias_words[0], cmd.span.clone());

                let mut new_args = vec![];
                for alias_arg in &alias_words[1..] {
                    new_args.push(colony_parser::Word::literal(alias_arg, cmd.span.clone()));
                }
                new_args.extend(cmd.args.clone());
                new_cmd.args = new_args;

                return Box::pin(self.execute_simple_command(&new_cmd)).await;
            }
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
                    let code = status.code();
                    self.context.set_last_exit_status(code);
                    Ok(code)
                }
                Statement::Pipeline(pipeline) => {
                    if pipeline.elements.len() == 1 {
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
                    Ok(0)
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
                Statement::BraceGroup(brace) => {
                    let mut last = 0;
                    for stmt in &brace.body {
                        last = self.execute_statement_recursive(stmt).await?;
                    }
                    Ok(last)
                }
                Statement::Subshell(subshell) => {
                    let mut last = 0;
                    for stmt in &subshell.body {
                        last = self.execute_statement_recursive(stmt).await?;
                    }
                    Ok(last)
                }
                Statement::FunctionDef(func_def) => {
                    self.context.define_function(func_def.name.clone(), func_def.clone());
                    Ok(0)
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
        match condition {
            colony_parser::Condition::Command(cmd) => {
                self.execute_statement_recursive(cmd).await
            }
            colony_parser::Condition::Expression(_) => {
                Ok(1)
            }
        }
    }
    
    async fn execute_while_internal(&mut self, while_stmt: &colony_parser::WhileStatement) -> Result<i32> {
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
}
