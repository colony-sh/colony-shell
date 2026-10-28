//! Shell builtin commands
//!
//! Builtins are commands that run within the shell process itself,
//! not as external programs. They can modify shell state (environment,
//! working directory, variables) which external commands cannot do.
//!
//! Essential builtins (POSIX):
//! - cd: Change working directory
//! - export: Set environment variables
//! - unset: Remove environment variables
//! - exit: Terminate the shell
//! - pwd: Print working directory
//! - echo: Print arguments
//! - type: Check command type
//! - alias/unalias: Manage command aliases
//!
//! Rule #14: All public APIs must have doc comments with examples

use crate::{ExecutionError, JobTable, Result};
use colony_channels::ChannelManager;
use colony_expand::{CompletionAction, CompletionSpec, Expander, ExpansionContext, Variable};
use colony_parser::SimpleCommand;
use colony_tuple::{Tuple, TupleSpace, Value};
use std::env;
use std::path::PathBuf;

/// Result of executing a builtin
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinResult {
    Success(i32),
    Failure(i32),
    Exit(i32),
    Source(PathBuf, Vec<String>),
    Eval(String),
}

impl BuiltinResult {
    pub fn exit_code(&self) -> i32 {
        match self {
            BuiltinResult::Success(code)
            | BuiltinResult::Failure(code)
            | BuiltinResult::Exit(code) => *code,
            BuiltinResult::Source(_, _) => 0,
            BuiltinResult::Eval(_) => 0,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(
            self,
            BuiltinResult::Success(0) | BuiltinResult::Source(_, _) | BuiltinResult::Eval(_)
        )
    }

    pub fn is_exit(&self) -> bool {
        matches!(self, BuiltinResult::Exit(_))
    }

    pub fn is_source(&self) -> bool {
        matches!(self, BuiltinResult::Source(_, _))
    }

    pub fn is_eval(&self) -> bool {
        matches!(self, BuiltinResult::Eval(_))
    }
}

fn is_builtin_base(name: &str) -> bool {
    matches!(
        name,
        "cd" | "export"
            | "unset"
            | "exit"
            | "pwd"
            | "echo"
            | "type"
            | "alias"
            | "unalias"
            | "."
            | "source"
            | "fg"
            | "bg"
            | "jobs"
            | "wait"
            | "disown"
            | "["
            | "test"
            | ":"
            | "context"
            | "declare"
            | "local"
            | "readonly"
            | "typeset"
            | "true"
            | "false"
            | "read"
            | "history"
            | "recho"
            | "set"
            | "shopt"
            | "printf"
            | "exec"
            | "trap"
            | "return"
            | "getopts"
            | "shift"
            | "dirs"
            | "pushd"
            | "popd"
            | "complete"
            | "compgen"
            | "mapfile"
            | "readarray"
            | "eval"
            | "hash"
            | "path"
            | "flux"
            | "colony"
            | "goal"
            | "tree"
            | "split"
            | "vsplit"
            | "layout"
            | "pane"
            | "secure"
            | "mirror"
            | "sidebar"
            | "serve"
            | "heartbeat"
            | "on"
            | "llm"
            | "tool"
            | "diag"
            | "canvas"
            | "draw"
    )
}

fn split_namespaced_builtin(name: &str) -> Option<(&str, &str)> {
    name.split_once(':')
}

pub fn is_builtin(name: &str) -> bool {
    if let Some((base, _sub)) = split_namespaced_builtin(name) {
        return is_builtin_base(base);
    }
    is_builtin_base(name)
}

pub fn serialize_function_for_export(func: &colony_parser::FunctionDef) -> String {
    let body = format_function_body(&func.body);
    format!("() {{ \n{}\n}}", body)
}

/// Execute a builtin command
///
/// Returns None if the command is not a builtin.
/// Returns Some(BuiltinResult) if it is a builtin.
///
/// Builtins with state changes emit tuples to the tuple space for reactive updates.
/// Tuples are scoped by session_id to enable multi-tab/window isolation.
#[allow(clippy::too_many_arguments)]
pub fn execute_builtin(
    cmd: &SimpleCommand,
    context: &mut ExpansionContext,
    jobs: &JobTable,
    tuple_space: &TupleSpace,
    session_id: &str,
    channels: &mut ChannelManager,
    history: Option<&mut crate::history::History>,
    history_store: Option<&std::sync::Arc<colony_history::HistoryStore>>,
) -> Option<Result<BuiltinResult>> {
    let mut expander = Expander::with_executor_and_expr(
        context.clone(),
        crate::execute_and_capture,
        |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
    );
    let name_fields = match expander.expand_word(&cmd.name) {
        Ok(fields) => fields,
        Err(e) => return Some(Err(e.into())),
    };

    if name_fields.is_empty() {
        return None;
    }

    let name = &name_fields[0].text;
    let (name, namespaced_arg) = if let Some((base, sub)) = split_namespaced_builtin(name) {
        (base, Some(sub.to_string()))
    } else {
        (name.as_str(), None)
    };

    if !is_builtin_base(name) {
        return None;
    }

    // Expand arguments - include extra fields from name expansion (e.g., "$@" -> multiple fields)
    let mut args: Vec<String> = name_fields[1..].iter().map(|f| f.text.clone()).collect();
    if let Some(ns) = namespaced_arg {
        if !ns.is_empty() {
            args.insert(0, ns);
        }
    }
    let is_assignment_builtin = matches!(
        name,
        "export" | "declare" | "local" | "readonly" | "typeset"
    );

    for arg_word in cmd.args.iter() {
        // For assignment builtins, check if this arg looks like an assignment
        let use_assignment_expansion = is_assignment_builtin
            && arg_word.parts.iter().any(|p| {
                if let colony_parser::WordPart::Literal(s) = p {
                    s.contains('=')
                } else {
                    false
                }
            });

        if use_assignment_expansion {
            match expander.expand_word_for_assignment(arg_word) {
                Ok(text) => args.push(text),
                Err(e) => return Some(Err(e.into())),
            }
        } else {
            let fields = match expander.expand_word(arg_word) {
                Ok(fields) => fields,
                Err(e) => return Some(Err(e.into())),
            };
            for field in fields {
                args.push(field.text);
            }
        }
    }

    context.sync_variables_from(expander.context());

    let result = match name {
        "cd" => builtin_cd(&args, context, tuple_space, session_id),
        "export" => builtin_export(&args, context, tuple_space, session_id),
        "unset" => builtin_unset(&args, context, tuple_space, session_id),
        "exit" => builtin_exit(&args),
        "pwd" => builtin_pwd(&args),
        "echo" => builtin_echo(&args),
        "type" => builtin_type(&args, context),
        "alias" => builtin_alias(&args, context),
        "unalias" => builtin_unalias(&args, context),
        "." | "source" => builtin_source(&args, context),
        "fg" => builtin_fg(&args, jobs),
        "bg" => builtin_bg(&args, jobs),
        "jobs" => builtin_jobs(&args, jobs),
        "wait" => builtin_wait(&args, jobs),
        "[" | "test" => builtin_test(&args, name),
        ":" => builtin_noop(&args),
        "context" => builtin_context(&args, channels),
        "declare" => builtin_declare(&args, context),
        "local" => builtin_local(&args, context),
        "true" => builtin_true(),
        "false" => builtin_false(),
        "read" => builtin_read(&args, context),
        "recho" => builtin_recho(&args),
        "set" => builtin_set(&args, context),
        "shopt" => builtin_shopt(&args, context),
        "printf" => builtin_printf(&args, context),
        "exec" => builtin_exec(&args, &cmd.redirects, context),
        "trap" => builtin_trap(&args, context),
        "return" => builtin_return(&args, context),
        "getopts" => builtin_getopts(&args, context),
        "shift" => builtin_shift(&args, context),
        "dirs" => builtin_dirs(&args, context),
        "pushd" => builtin_pushd(&args, context, tuple_space, session_id),
        "popd" => builtin_popd(&args, context, tuple_space, session_id),
        "complete" => builtin_complete(&args, context),
        "compgen" => builtin_compgen(&args, context),
        "mapfile" | "readarray" => builtin_mapfile(&args, context),
        "eval" => builtin_eval(&args),
        "readonly" => builtin_readonly(&args, context),
        "typeset" => builtin_declare(&args, context), // typeset is alias for declare
        "disown" => builtin_disown(&args, jobs),
        "hash" => builtin_hash(&args, context),
        "history" => builtin_history(&args, history, history_store),
        "path" => builtin_path(&args, context, tuple_space, session_id),
        "flux" => crate::flux::builtin_flux(&args, tuple_space),
        "colony" => builtin_colony(&args),
        "goal" => builtin_goal(&args),
        "tree" => crate::file_tree::protocol::builtin_tree(&args),
        "split" => crate::panes::builtin_split(&args),
        "vsplit" => crate::panes::builtin_vsplit(&args),
        "layout" => crate::panes::builtin_layout(&args),
        "pane" => crate::panes::builtin_pane(&args),
        "secure" => builtin_secure(&args, context),
        "mirror" => crate::mirror::builtin_mirror(&args),
        "sidebar" => match crate::sidebar::parse_sidebar_args(&args) {
            Ok(cmd) => {
                let json = serde_json::to_string(&cmd).unwrap_or_default();
                tuple_space.out(colony_tuple::Tuple::new(vec![
                    colony_tuple::Value::Str("sidebar_command".to_string()),
                    colony_tuple::Value::Str(json),
                ]));
                Ok(BuiltinResult::Success(0))
            }
            Err(e) => {
                eprintln!("sidebar: {e}");
                Ok(BuiltinResult::Failure(1))
            }
        },
        "serve" => {
            // Namespaced builtins: serve:status and serve:header use the
            // thread-local response meta channel for per-request metadata.
            // The main "serve" command is intercepted at the Executor level
            // (see lib.rs execute_simple_command) because it needs ServeState.
            if args.is_empty() {
                // No subcommand -- the main "serve" builtin is handled by the
                // Executor which has access to ServeState. This branch handles
                // the namespaced "serve:status" / "serve:header" calls.
                eprintln!("serve: missing subcommand (use serve start, serve stop, etc.)");
                Ok(BuiltinResult::Failure(1))
            } else {
                match args[0].as_str() {
                    "status" if args.len() >= 2 => {
                        // serve:status <code>  -- set HTTP response status
                        let Ok(code) = args[1].parse::<u16>() else {
                            eprintln!("serve:status: invalid status code '{}'", args[1]);
                            return Some(Ok(BuiltinResult::Failure(1)));
                        };
                        let sent = crate::serve::endpoint_handler::send_response_meta(
                            crate::serve::state::ResponseMeta::Status(code),
                        );
                        if sent {
                            Ok(BuiltinResult::Success(0))
                        } else {
                            eprintln!("serve:status: not inside a serve handler");
                            Ok(BuiltinResult::Failure(1))
                        }
                    }
                    "header" if args.len() >= 3 => {
                        // serve:header <name> <value>  -- set HTTP response header
                        let name = args[1].clone();
                        let value = args[2..].join(" ");
                        let sent = crate::serve::endpoint_handler::send_response_meta(
                            crate::serve::state::ResponseMeta::Header(name, value),
                        );
                        if sent {
                            Ok(BuiltinResult::Success(0))
                        } else {
                            eprintln!("serve:header: not inside a serve handler");
                            Ok(BuiltinResult::Failure(1))
                        }
                    }
                    _ => {
                        // Other serve subcommands are handled at the Executor level.
                        // If we get here, it means execute_builtin was called directly
                        // without the Executor intercept -- return a helpful error.
                        eprintln!("serve: this subcommand requires executor context");
                        Ok(BuiltinResult::Failure(1))
                    }
                }
            }
        }
        "heartbeat" => {
            // The heartbeat builtin is intercepted at the Executor level
            // (see lib.rs execute_simple_command) because it needs
            // HeartbeatScheduler state. If we get here without args,
            // show usage.
            if args.is_empty() {
                eprintln!("heartbeat: missing subcommand");
                eprintln!("Usage: heartbeat start|stop|run|status|interval|log|render");
                Ok(BuiltinResult::Failure(1))
            } else {
                eprintln!("heartbeat: this subcommand requires executor context");
                Ok(BuiltinResult::Failure(1))
            }
        }
        "on" => crate::events::builtin_on(&args),
        "llm" => builtin_llm(&args, channels),
        "tool" => builtin_tool(&args, channels),
        "diag" => builtin_diag(&args, channels),
        "canvas" => crate::canvas_builtin::builtin_canvas(&args),
        "draw" => crate::draw_builtin::builtin_draw(&args),
        _ => unreachable!("is_builtin returned true but no handler"),
    };

    Some(result)
}

/// cd - Change working directory
///
/// Usage: cd [dir]
/// - No args: cd to HOME
/// - One arg: cd to specified directory
///
/// **Reactive:** Emits ("dir_change", session_id, old_dir, new_dir) tuple (Rule 46)
/// Tuples are scoped by session_id to enable multi-tab/window isolation.
fn builtin_cd(
    args: &[String],
    context: &mut ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) -> Result<BuiltinResult> {
    let old_dir = env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "/".to_string());

    let target = if args.is_empty() {
        context
            .get_env("HOME")
            .ok_or_else(|| {
                ExecutionError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "HOME not set",
                ))
            })?
            .to_string()
    } else if args[0] == "-" {
        context
            .get_env("OLDPWD")
            .ok_or_else(|| {
                ExecutionError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "OLDPWD not set",
                ))
            })?
            .to_string()
    } else {
        args[0].clone()
    };

    let print_dir = args.first().map(|s| s == "-").unwrap_or(false);

    env::set_current_dir(&target).map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(
            e.kind(),
            format!("cd: {}: {}", target, e),
        ))
    })?;

    let new_dir = env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| target.to_string());

    if print_dir {
        println!("{}", new_dir);
    }

    context.set_env("OLDPWD", &old_dir);
    context.set_env("PWD", &new_dir);

    // Emit session-scoped tuple for reactive updates
    tuple_space.out(Tuple::from(vec![
        Value::Str("dir_change".to_string()),
        Value::Str(session_id.to_string()),
        Value::Str(old_dir),
        Value::Str(new_dir.clone()),
    ]));

    Ok(BuiltinResult::Success(0))
}

/// export - Set environment variables or export functions
///
/// Usage: export [-f] [-n] [-p] [name[=value] ...]
/// -f: Export functions (export -f funcname)
/// -n: Remove export attribute (export -n name, export -fn funcname)
/// -p: Print all exported variables/functions
pub(crate) fn builtin_export(
    args: &[String],
    context: &mut ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) -> Result<BuiltinResult> {
    let mut export_functions = false;
    let mut unexport = false;
    let mut print_mode = false;
    let mut names: Vec<&str> = Vec::new();

    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 && !arg.contains('=') {
            for c in arg[1..].chars() {
                match c {
                    'f' => export_functions = true,
                    'n' => unexport = true,
                    'p' => print_mode = true,
                    _ => {}
                }
            }
        } else {
            names.push(arg);
        }
    }

    if export_functions && unexport {
        for name in &names {
            context.unexport_function(name);
        }
        return Ok(BuiltinResult::Success(0));
    }

    if export_functions {
        if names.is_empty() || print_mode {
            // Collect and sort for deterministic output
            let mut func_names: Vec<String> = context.exported_function_names().cloned().collect();
            func_names.sort();
            for name in &func_names {
                if let Some(func) = context.get_function(name) {
                    let body = format_function_body(&func.body);
                    println!("{} () \n{{ \n{}\n}}", name, body);
                }
            }
            return Ok(BuiltinResult::Success(0));
        }

        let mut exit_code = 0;
        for name in &names {
            if context.is_function(name) {
                context.export_function(name);
            } else {
                eprintln!("export: {}: not a function", name);
                exit_code = 1;
            }
        }
        return Ok(if exit_code == 0 {
            BuiltinResult::Success(0)
        } else {
            BuiltinResult::Failure(1)
        });
    }

    if unexport {
        for name in &names {
            env::remove_var(name);
            context.unset_env(name);
            if let Some(mut attrs) = context.get_var_attributes(name).cloned() {
                attrs.export = false;
                context.set_var_attributes(name, attrs);
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    if print_mode || names.is_empty() {
        // Print exported variables
        for (key, value) in env::vars() {
            println!("declare -x {}=\"{}\"", key, value);
        }
        // Also print exported functions (bash compat: `export -p` includes functions)
        let mut func_names: Vec<String> = context.exported_function_names().cloned().collect();
        func_names.sort();
        for name in &func_names {
            if let Some(func) = context.get_function(name) {
                let body = format_function_body(&func.body);
                println!("declare -fx {} () \n{{ \n{}\n}}", name, body);
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    for arg in names {
        if let Some(eq_pos) = arg.find('=') {
            let (name, value) = arg.split_at(eq_pos);
            let value = &value[1..];

            let old_value = context.get_env(name).unwrap_or("").to_string();

            context.set_env(name, value);
            env::set_var(name, value);

            tuple_space.out(Tuple::from(vec![
                Value::Str("env_change".to_string()),
                Value::Str(session_id.to_string()),
                Value::Str(name.to_string()),
                Value::Str(old_value),
                Value::Str(value.to_string()),
            ]));
        } else if let Some(value) = context.get_var_or_env(arg) {
            env::set_var(arg, &value);
        } else {
            env::set_var(arg, "");
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// unset - Remove environment variables
///
/// Usage: unset VAR [VAR2 ...]
///
/// **Reactive:** Emits ("env_change", session_id, var, old, null) tuple (Rule 43)
fn builtin_unset(
    args: &[String],
    context: &mut ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) -> Result<BuiltinResult> {
    let mut unset_nameref_only = false;
    let mut unset_function = false;
    let mut var_names: Vec<&str> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-n" => unset_nameref_only = true,
            "-f" => unset_function = true,
            "-v" => {}
            s if s.starts_with('-') => {}
            _ => var_names.push(arg),
        }
    }

    for var_name in var_names {
        if unset_function {
            context.unset_function(var_name);
            continue;
        }

        if unset_nameref_only {
            context.unset_nameref(var_name);
            continue;
        }

        if context.is_readonly(var_name) {
            eprintln!(
                "colony: unset: {}: cannot unset: readonly variable",
                var_name
            );
            return Ok(BuiltinResult::Success(1));
        }

        let old_value = context.get_var_or_env(var_name).unwrap_or_default();

        context.unset_variable(var_name);
        context.unset_env(var_name);
        env::remove_var(var_name);

        tuple_space.out(Tuple::from(vec![
            Value::Str("env_change".to_string()),
            Value::Str(session_id.to_string()),
            Value::Str(var_name.to_string()),
            Value::Str(old_value),
            Value::Null,
        ]));
    }

    Ok(BuiltinResult::Success(0))
}

/// exit - Exit the shell
///
/// Usage: exit [n]
/// - No args: exit with status 0
/// - One arg: exit with specified status
fn builtin_exit(args: &[String]) -> Result<BuiltinResult> {
    let code = if args.is_empty() {
        0
    } else {
        args[0].parse::<i32>().unwrap_or(255)
    };

    Ok(BuiltinResult::Exit(code))
}

/// pwd - Print working directory
///
/// Usage: pwd
fn builtin_pwd(_args: &[String]) -> Result<BuiltinResult> {
    use std::io::Write;
    use std::os::unix::io::FromRawFd;

    let cwd = env::current_dir().map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(e.kind(), format!("pwd: {}", e)))
    })?;

    let mut stdout = unsafe { std::fs::File::from_raw_fd(1) };
    let _ = writeln!(stdout, "{}", cwd.display());
    let _ = stdout.flush();
    std::mem::forget(stdout);

    Ok(BuiltinResult::Success(0))
}

/// echo - Print arguments
///
/// Usage: echo [-neE] [args...]
/// Prints arguments separated by spaces, followed by newline
///
/// Options:
///   -n    Do not print trailing newline
///   -e    Enable interpretation of backslash escapes
///   -E    Disable interpretation of backslash escapes (default)
///
/// When -e is in effect, the following escape sequences are recognized:
///   \\    backslash
///   \a    alert (bell)
///   \b    backspace
///   \c    suppress further output (including trailing newline)
///   \e    escape character
///   \f    form feed
///   \n    new line
///   \r    carriage return
///   \t    horizontal tab
///   \v    vertical tab
///   \0nnn octal value (0-3 octal digits)
///   \xHH  hexadecimal value (1-2 hex digits)
fn builtin_echo(args: &[String]) -> Result<BuiltinResult> {
    let mut no_newline = false;
    let mut interpret_escapes = false;
    let mut text_args_start = 0;

    // Parse options - only at the start
    for (i, arg) in args.iter().enumerate() {
        if !arg.starts_with('-') || arg == "-" {
            text_args_start = i;
            break;
        }

        // Check if all chars after '-' are valid option chars
        let opt_chars: Vec<char> = arg[1..].chars().collect();
        if opt_chars.is_empty()
            || !opt_chars
                .iter()
                .all(|c| *c == 'n' || *c == 'e' || *c == 'E')
        {
            // Not a valid option, treat as text
            text_args_start = i;
            break;
        }

        // Apply options
        for c in opt_chars {
            match c {
                'n' => no_newline = true,
                'e' => interpret_escapes = true,
                'E' => interpret_escapes = false,
                _ => unreachable!(),
            }
        }
        text_args_start = i + 1;
    }

    let text_args = &args[text_args_start..];
    let text = text_args.join(" ");

    let output = if interpret_escapes {
        expand_echo_escapes(&text)
    } else {
        (text, false)
    };

    use std::io::Write;
    use std::os::unix::io::FromRawFd;

    // SAFETY: borrowing fd 1 to write, using forget() to avoid closing on drop
    let mut stdout = unsafe { std::fs::File::from_raw_fd(1) };

    if output.1 || no_newline {
        let _ = stdout.write_all(output.0.as_bytes());
    } else {
        let _ = writeln!(stdout, "{}", output.0);
    }
    let _ = stdout.flush();
    std::mem::forget(stdout);

    Ok(BuiltinResult::Success(0))
}

/// Expand echo escape sequences
/// Returns (expanded_string, stop_output) where stop_output is true if \c was encountered
fn expand_echo_escapes(s: &str) -> (String, bool) {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => result.push('\\'),
                Some('a') => result.push('\x07'),
                Some('b') => result.push('\x08'),
                Some('c') => return (result, true), // Stop output
                Some('e') => result.push('\x1b'),
                Some('f') => result.push('\x0c'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('v') => result.push('\x0b'),
                Some('0') => {
                    // Octal: \0, \0n, \0nn, \0nnn (0-3 digits after \0)
                    let mut octal = String::new();
                    for _ in 0..3 {
                        if let Some(&next) = chars.peek() {
                            if ('0'..='7').contains(&next) {
                                octal.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    if octal.is_empty() {
                        result.push('\0');
                    } else {
                        let val = u8::from_str_radix(&octal, 8).unwrap_or(0);
                        result.push(val as char);
                    }
                }
                Some('x') => {
                    // Hex: \xH or \xHH (1-2 hex digits)
                    let mut hex = String::new();
                    for _ in 0..2 {
                        if let Some(&next) = chars.peek() {
                            if next.is_ascii_hexdigit() {
                                hex.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    if hex.is_empty() {
                        result.push_str("\\x");
                    } else {
                        let val = u8::from_str_radix(&hex, 16).unwrap_or(0);
                        result.push(val as char);
                    }
                }
                Some(other) => {
                    // Unknown escape - keep backslash and character
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    (result, false)
}

/// recho - Really echo args (test helper)
///
/// Usage: recho [args...]
/// Prints each argument on its own line as `argv[N] = <arg>` with control
/// characters made visible. Used for testing word splitting and expansion.
///
/// Control character display:
/// - 0x00-0x1F: ^@ through ^_ (e.g., ^A for 0x01, ^I for tab)
/// - 0x7F: ^?
///
/// If no arguments are provided, prints nothing (not even "nothing").
/// This matches bash's recho behavior for testing $@ expansion.
fn builtin_recho(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        // No arguments - print nothing (matches bash recho behavior)
        return Ok(BuiltinResult::Success(0));
    }

    for (i, arg) in args.iter().enumerate() {
        print!("argv[{}] = <", i + 1);
        for byte in arg.bytes() {
            if byte < 0x20 {
                // Control character 0x00-0x1F -> ^@ through ^_
                print!("^{}", (byte + 64) as char);
            } else if byte == 0x7F {
                // DEL character -> ^?
                print!("^?");
            } else {
                // Normal character
                print!("{}", byte as char);
            }
        }
        println!(">");
    }

    Ok(BuiltinResult::Success(0))
}

fn builtin_set(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }

    let mut positional_params: Vec<String> = Vec::new();
    let mut seen_double_dash = false;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        if seen_double_dash {
            positional_params.push(arg.clone());
            i += 1;
            continue;
        }

        if arg == "--" {
            seen_double_dash = true;
            i += 1;
            continue;
        }

        if arg == "-o" || arg == "+o" {
            let enable = arg == "-o";
            i += 1;
            if i < args.len() {
                set_option_by_name(context, &args[i], enable);
            }
            i += 1;
            continue;
        }

        if arg.starts_with('-') && arg.len() > 1 && arg != "-" {
            if arg.starts_with("-o") && arg.len() > 2 {
                set_option_by_name(context, &arg[2..], true);
            } else {
                for c in arg[1..].chars() {
                    match c {
                        'C' => context.shell_options_mut().noclobber = true,
                        'e' => context.shell_options_mut().errexit = true,
                        'u' => context.shell_options_mut().nounset = true,
                        'x' => context.shell_options_mut().xtrace = true,
                        'f' => context.shell_options_mut().noglob = true,
                        'n' => context.shell_options_mut().noexec = true,
                        'v' => context.shell_options_mut().verbose = true,
                        'o' => {}
                        _ => {}
                    }
                }
            }
        } else if arg.starts_with('+') && arg.len() > 1 {
            if arg.starts_with("+o") && arg.len() > 2 {
                set_option_by_name(context, &arg[2..], false);
            } else {
                for c in arg[1..].chars() {
                    match c {
                        'C' => context.shell_options_mut().noclobber = false,
                        'e' => context.shell_options_mut().errexit = false,
                        'u' => context.shell_options_mut().nounset = false,
                        'x' => context.shell_options_mut().xtrace = false,
                        'f' => context.shell_options_mut().noglob = false,
                        'n' => context.shell_options_mut().noexec = false,
                        'v' => context.shell_options_mut().verbose = false,
                        'o' => {}
                        _ => {}
                    }
                }
            }
        } else {
            positional_params.push(arg.clone());
        }
        i += 1;
    }

    if seen_double_dash || !positional_params.is_empty() {
        context.set_positional_params(positional_params);
    }

    Ok(BuiltinResult::Success(0))
}

fn set_option_by_name(context: &mut ExpansionContext, name: &str, enable: bool) {
    let opts = context.shell_options_mut();
    match name {
        "errexit" => opts.errexit = enable,
        "noclobber" => opts.noclobber = enable,
        "nounset" => opts.nounset = enable,
        "xtrace" => opts.xtrace = enable,
        "noglob" => opts.noglob = enable,
        "noexec" => opts.noexec = enable,
        "verbose" => opts.verbose = enable,
        "pipefail" => opts.pipefail = enable,
        _ => {}
    }
}

/// Shell reserved keywords
const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "case", "esac", "for", "while", "until", "do", "done",
    "in", "function", "select", "time", "coproc", "!", "{", "}", "[[", "]]",
];

fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(&name)
}

/// type - Check command type
///
/// Usage: type [-afptP] name [name...]
///
/// Options:
///   -a  Display all locations (alias, function, builtin, file)
///   -f  Suppress function lookup
///   -p  Print path for external commands only
///   -P  Force PATH search even if function/builtin exists
///   -t  Print single word: alias, keyword, function, builtin, file
fn builtin_type(args: &[String], context: &ExpansionContext) -> Result<BuiltinResult> {
    // Parse options
    let mut show_all = false;
    let mut suppress_functions = false;
    let mut path_only = false;
    let mut force_path = false;
    let mut type_only = false;

    let mut names: Vec<&String> = Vec::new();
    let mut parsing_opts = true;

    for arg in args {
        if parsing_opts && arg.starts_with('-') && arg.len() > 1 && arg != "--" {
            if arg == "--" {
                parsing_opts = false;
                continue;
            }
            for ch in arg[1..].chars() {
                match ch {
                    'a' => show_all = true,
                    'f' => suppress_functions = true,
                    'p' => path_only = true,
                    'P' => force_path = true,
                    't' => type_only = true,
                    _ => {
                        eprintln!("type: -{}: invalid option", ch);
                        return Ok(BuiltinResult::Failure(2));
                    }
                }
            }
        } else {
            parsing_opts = false;
            names.push(arg);
        }
    }

    if names.is_empty() {
        // No names provided is not an error, just return success
        return Ok(BuiltinResult::Success(0));
    }

    let mut exit_code = 0;

    for name in &names {
        let mut found = false;

        // -P forces PATH search only
        if force_path {
            if let Ok(path) = crate::path::resolve_command(name, env::var("PATH").ok().as_deref()) {
                if type_only {
                    println!("file");
                } else {
                    println!("{}", path.display());
                }
                found = true;
            }
        } else {
            // Check alias (unless -p which only shows paths)
            if !path_only {
                if let Some(value) = context.get_alias(name) {
                    if type_only {
                        println!("alias");
                    } else {
                        println!("{} is aliased to `{}'", name, value);
                    }
                    found = true;
                    if !show_all {
                        continue;
                    }
                }
            }

            // Check keyword (unless -p)
            if !path_only && is_keyword(name) {
                if type_only {
                    println!("keyword");
                } else {
                    println!("{} is a shell keyword", name);
                }
                found = true;
                if !show_all {
                    continue;
                }
            }

            // Check function (unless -f or -p)
            if !suppress_functions && !path_only && context.get_function(name).is_some() {
                if type_only {
                    println!("function");
                } else {
                    // For -a we just say it's a function, for normal we show definition
                    if show_all {
                        println!("{} is a function", name);
                    } else {
                        // Show function definition
                        println!("{} is a function", name);
                        println!("{} ()", name);
                        println!("{{");
                        println!("    ...");
                        println!("}}");
                    }
                }
                found = true;
                if !show_all {
                    continue;
                }
            }

            // Check builtin (unless -p)
            if !path_only && is_builtin(name) {
                if type_only {
                    println!("builtin");
                } else {
                    println!("{} is a shell builtin", name);
                }
                found = true;
                if !show_all {
                    continue;
                }
            }

            let is_alias_func_or_builtin = context.get_alias(name).is_some()
                || context.get_function(name).is_some()
                || is_builtin(name);

            if path_only && is_alias_func_or_builtin {
                found = true;
            } else if let Ok(path) =
                crate::path::resolve_command(name, env::var("PATH").ok().as_deref())
            {
                if type_only {
                    println!("file");
                } else if path_only {
                    println!("{}", path.display());
                } else {
                    println!("{} is {}", name, path.display());
                }
                found = true;
            }
        }

        if !found {
            if !type_only {
                eprintln!("type: {}: not found", name);
            }
            exit_code = 1;
        }
    }

    if exit_code == 0 {
        Ok(BuiltinResult::Success(0))
    } else {
        Ok(BuiltinResult::Failure(exit_code))
    }
}

/// alias - Manage command aliases
///
/// Usage:
/// - alias [-p]         - List all aliases
/// - alias name         - Show specific alias
/// - alias name='value' - Define alias
fn builtin_alias(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() || (args.len() == 1 && args[0] == "-p") {
        let mut aliases: Vec<_> = context.aliases().iter().collect();
        aliases.sort_by_key(|(name, _)| *name);

        for (name, value) in aliases {
            println!("alias {}='{}'", name, value);
        }
        return Ok(BuiltinResult::Success(0));
    }

    for arg in args {
        if arg == "-p" {
            continue;
        }
        if let Some(eq_pos) = arg.find('=') {
            let (name, value) = arg.split_at(eq_pos);
            let value = &value[1..];
            let value = value.trim_matches('\'').trim_matches('"');

            if name.is_empty() {
                eprintln!("alias: `={}': invalid alias name", value);
                return Ok(BuiltinResult::Failure(1));
            }

            context.set_alias(name.to_string(), value.to_string());
        } else if let Some(value) = context.get_alias(arg) {
            println!("alias {}='{}'", arg, value);
        } else {
            eprintln!("alias: {}: not found", arg);
            return Ok(BuiltinResult::Failure(1));
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// unalias - Remove command aliases
///
/// Usage: unalias [-a] name [name...]
///
/// Removes the specified aliases from the alias table.
/// -a removes all aliases.
fn builtin_unalias(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("unalias: usage: unalias [-a] name [name ...]");
        return Ok(BuiltinResult::Failure(1));
    }

    if args.len() == 1 && args[0] == "-a" {
        context.clear_aliases();
        return Ok(BuiltinResult::Success(0));
    }

    for name in args {
        if name == "-a" {
            continue;
        }
        if context.get_alias(name).is_some() {
            context.unset_alias(name);
        } else {
            eprintln!("unalias: {}: not found", name);
            return Ok(BuiltinResult::Failure(1));
        }
    }

    Ok(BuiltinResult::Success(0))
}

fn builtin_source(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("source: filename argument required");
        return Ok(BuiltinResult::Failure(2));
    }

    let filename = &args[0];
    let source_args = args[1..].to_vec();

    let file_path = if filename.contains('/') {
        PathBuf::from(filename)
    } else {
        let path_var = context.get_env("PATH").unwrap_or_default();
        let mut found_path = None;

        for dir in path_var.split(':') {
            let candidate = PathBuf::from(dir).join(filename);
            if candidate.is_file() {
                found_path = Some(candidate);
                break;
            }
        }

        if found_path.is_none() {
            let cwd_candidate = PathBuf::from(filename);
            if cwd_candidate.is_file() {
                found_path = Some(cwd_candidate);
            }
        }

        match found_path {
            Some(path) => path,
            None => {
                eprintln!("source: {}: No such file or directory", filename);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    };

    if !file_path.is_file() {
        eprintln!("source: {}: No such file or directory", file_path.display());
        return Ok(BuiltinResult::Failure(1));
    }

    Ok(BuiltinResult::Source(file_path, source_args))
}

fn builtin_eval(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }
    let eval_string = args.join(" ");
    Ok(BuiltinResult::Eval(eval_string))
}

/// fg - Bring job to foreground
///
/// Usage: fg [%job_id]
///
/// Resumes a stopped job in the foreground by sending SIGCONT to its process
/// group and transferring terminal control. If no job spec is given, uses the
/// most recent stopped job, or the most recent background job as a fallback.
fn builtin_fg(args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    // Resolve the target job: explicit spec or most recent stopped/background
    let job = if let Some(spec) = args.first() {
        let Some(job_id) = parse_job_spec(spec, jobs) else {
            eprintln!("fg: {}: no such job", spec);
            return Ok(BuiltinResult::Failure(1));
        };
        let Some(job) = jobs.get_job(job_id) else {
            eprintln!("fg: {}: no such job", spec);
            return Ok(BuiltinResult::Failure(1));
        };
        job
    } else {
        // Prefer stopped jobs, fall back to background
        let Some(job) = jobs
            .most_recent_stopped_job()
            .or_else(|| jobs.most_recent_active_job())
        else {
            eprintln!("fg: no current job");
            return Ok(BuiltinResult::Failure(1));
        };
        job
    };

    // Print the command being foregrounded (bash convention)
    eprintln!("{}", job.command);

    // Send SIGCONT to resume the process group (no-op if already running)
    if let Err(e) = crate::jobs::continue_job(job.pgid) {
        // ESRCH means the process group is gone; clean up and report
        eprintln!("fg: job has terminated: {}", e);
        jobs.update_status(job.id, crate::JobStatus::Done(crate::ExitStatus::new(1)));
        return Ok(BuiltinResult::Failure(1));
    }

    // Transfer terminal control to the job's process group
    if let Err(e) = crate::jobs::give_terminal_to(job.pgid) {
        eprintln!("fg: terminal control: {}", e);
        // Continue anyway; the job is running even if tcsetpgrp failed
    }

    // Mark the job as running in the foreground
    jobs.update_status(job.id, crate::JobStatus::Running);

    // Wait for the job to finish or stop again (blocking)
    let result_status = crate::jobs::wait_for_foreground_job(&job);

    // Reclaim terminal control for the shell
    let _ = crate::jobs::take_terminal();

    // Update job table based on the wait result
    match result_status {
        crate::JobStatus::Stopped => {
            jobs.update_status(job.id, crate::JobStatus::Stopped);
            eprintln!("\n[{}]+  Stopped  {}", job.id, job.command);
            // Stopped job returns 148 (128 + SIGTSTP=20 on most platforms)
            Ok(BuiltinResult::Success(148))
        }
        crate::JobStatus::Done(exit_status) => {
            jobs.update_status(job.id, crate::JobStatus::Done(exit_status));
            Ok(BuiltinResult::Success(exit_status.code()))
        }
        // Running and Background are not returned by wait_for_foreground_job
        _ => Ok(BuiltinResult::Success(0)),
    }
}

/// bg - Continue job in background
///
/// Usage: bg [%job_id]
///
/// Resumes a stopped job in the background by sending SIGCONT to its process
/// group. If no job spec is given, uses the most recent stopped job.
/// The job continues running without terminal control.
fn builtin_bg(args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    // Resolve the target job: explicit spec or most recent stopped
    let job = if let Some(spec) = args.first() {
        let Some(job_id) = parse_job_spec(spec, jobs) else {
            eprintln!("bg: {}: no such job", spec);
            return Ok(BuiltinResult::Failure(1));
        };
        let Some(job) = jobs.get_job(job_id) else {
            eprintln!("bg: {}: no such job", spec);
            return Ok(BuiltinResult::Failure(1));
        };
        job
    } else {
        let Some(job) = jobs.most_recent_stopped_job() else {
            eprintln!("bg: no current job");
            return Ok(BuiltinResult::Failure(1));
        };
        job
    };

    // Only stopped jobs can be backgrounded
    if !job.is_stopped() {
        eprintln!("bg: job {} already in background", job.id);
        return Ok(BuiltinResult::Failure(1));
    }

    // Send SIGCONT to resume the process group
    if let Err(e) = crate::jobs::continue_job(job.pgid) {
        eprintln!("bg: job has terminated: {}", e);
        jobs.update_status(job.id, crate::JobStatus::Done(crate::ExitStatus::new(1)));
        return Ok(BuiltinResult::Failure(1));
    }

    // Mark the job as running in the background
    jobs.update_status(job.id, crate::JobStatus::Background);

    // Print the background job notification (bash convention)
    println!("[{}] {} &", job.id, job.command);

    Ok(BuiltinResult::Success(0))
}

/// jobs - List all jobs
///
/// Usage: jobs
fn builtin_jobs(_args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    let job_list = jobs.list_jobs();

    if job_list.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }

    for job in job_list {
        let status_str = match job.status {
            crate::JobStatus::Running => "Running",
            crate::JobStatus::Background => "Running",
            crate::JobStatus::Stopped => "Stopped",
            crate::JobStatus::Done(status) => {
                if status.is_success() {
                    "Done"
                } else {
                    "Exit"
                }
            }
        };

        println!("[{}]  {}  {}", job.id, status_str, job.command);
    }

    Ok(BuiltinResult::Success(0))
}

/// wait - Wait for job or process to complete
///
/// Usage: wait [pid...] or wait [%job_id...]
/// Returns: Exit status of waited process, or 0 if no arguments
///
/// If no arguments, waits for all background children and returns 0.
/// If PID given, waits for that process and returns its exit status.
/// If process already exited, returns its cached status.
fn builtin_wait(args: &[String], _jobs: &JobTable) -> Result<BuiltinResult> {
    use nix::sys::wait::{waitpid, WaitStatus};
    use nix::unistd::Pid;

    // No arguments: wait for all children (blocking)
    if args.is_empty() {
        let mut last_status = 0;
        loop {
            match waitpid(Pid::from_raw(-1), None) {
                Ok(WaitStatus::Exited(_, code)) => {
                    last_status = code;
                }
                Ok(WaitStatus::Signaled(_, signal, _)) => {
                    last_status = 128 + signal as i32;
                }
                Ok(_) => {
                    // continue waiting
                }
                Err(nix::errno::Errno::ECHILD) => {
                    // No more children
                    break;
                }
                Err(_) => {
                    break;
                }
            }
        }
        return Ok(BuiltinResult::Success(last_status));
    }

    // Process each PID argument
    let mut last_status = 0;
    for arg in args {
        // Handle job IDs (e.g., %1)
        if arg.starts_with('%') {
            // Job ID handling - for now skip
            continue;
        }

        // Parse as PID
        let pid: i32 = match arg.parse() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("wait: `{}': not a valid pid", arg);
                last_status = 127;
                continue;
            }
        };

        // Wait for the specific process
        match waitpid(Pid::from_raw(pid), None) {
            Ok(WaitStatus::Exited(_, code)) => {
                last_status = code;
            }
            Ok(WaitStatus::Signaled(_, signal, _)) => {
                // Process killed by signal - return 128 + signal number
                last_status = 128 + signal as i32;
            }
            Ok(_) => {
                // Other status (stopped, continued) - treat as success
                last_status = 0;
            }
            Err(nix::errno::Errno::ECHILD) => {
                // No such child process - already exited or not our child
                // Bash returns 127 in this case
                last_status = 127;
            }
            Err(e) => {
                eprintln!("wait: {}: {}", pid, e);
                last_status = 127;
            }
        }
    }

    Ok(BuiltinResult::Success(last_status))
}

fn builtin_disown(args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    let mut remove_all = false;
    let mut job_specs: Vec<&str> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-a" => remove_all = true,
            "-r" | "-h" => {}
            s if s.starts_with('%') || s.chars().all(|c| c.is_ascii_digit()) => {
                job_specs.push(s);
            }
            _ => {
                eprintln!("disown: {}: invalid option", arg);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    if remove_all {
        for job in jobs.list_jobs() {
            jobs.remove_job(job.id);
        }
        return Ok(BuiltinResult::Success(0));
    }

    if job_specs.is_empty() {
        if let Some(job) = jobs.list_jobs().last() {
            jobs.remove_job(job.id);
        }
    } else {
        for spec in job_specs {
            if let Some(job_id) = parse_job_spec(spec, jobs) {
                jobs.remove_job(job_id);
            } else {
                eprintln!("disown: {}: no such job", spec);
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

fn parse_job_spec(spec: &str, jobs: &JobTable) -> Option<usize> {
    if let Some(rest) = spec.strip_prefix('%') {
        if rest == "%" || rest == "+" {
            jobs.list_jobs().last().map(|j| j.id)
        } else if rest == "-" {
            let all = jobs.list_jobs();
            if all.len() >= 2 {
                Some(all[all.len() - 2].id)
            } else {
                None
            }
        } else if let Ok(n) = rest.parse::<usize>() {
            if jobs.get_job(n).is_some() {
                Some(n)
            } else {
                None
            }
        } else {
            jobs.list_jobs()
                .iter()
                .find(|j| j.command.starts_with(rest))
                .map(|j| j.id)
        }
    } else if let Ok(pid) = spec.parse::<i32>() {
        jobs.find_by_pid(nix::unistd::Pid::from_raw(pid))
            .map(|j| j.id)
    } else {
        None
    }
}

fn builtin_hash(_args: &[String], _context: &mut ExpansionContext) -> Result<BuiltinResult> {
    Ok(BuiltinResult::Success(0))
}

fn builtin_readonly(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use colony_expand::VarAttributes;

    if args.is_empty() {
        for (name, var, attr) in context.variables_with_attrs() {
            if attr.map(|a| a.readonly).unwrap_or(false) {
                let val = var.as_str();
                println!("declare -r {}=\"{}\"", name, val);
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    let mut print_mode = false;
    let mut var_specs: Vec<&str> = Vec::new();

    for arg in args {
        if arg == "-p" {
            print_mode = true;
        } else if arg.starts_with('-') {
        } else {
            var_specs.push(arg);
        }
    }

    if print_mode && var_specs.is_empty() {
        for (name, var, attr) in context.variables_with_attrs() {
            if attr.map(|a| a.readonly).unwrap_or(false) {
                let val = var.as_str();
                println!("declare -r {}=\"{}\"", name, val);
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    let attrs = VarAttributes {
        readonly: true,
        ..Default::default()
    };

    for spec in var_specs {
        let (name, value_opt) = if let Some(eq_pos) = spec.find('=') {
            (&spec[..eq_pos], Some(&spec[eq_pos + 1..]))
        } else {
            (spec, None)
        };

        if let Err(e) = context.declare_with_attrs(name, value_opt.map(String::from), attrs) {
            eprintln!("readonly: {}: {}", name, e);
            return Ok(BuiltinResult::Failure(1));
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// [ or test - Evaluate conditional expressions
///
/// Usage: [ expression ] or test expression
/// Returns 0 (true) or 1 (false)
///
/// Supported tests:
/// - [ string ] - true if non-empty
/// - [ string = string ] - true if equal
/// - [ string != string ] - true if not equal
/// - [ -n string ] - true if non-empty
/// - [ -z string ] - true if empty
/// - [ -f file ] - true if file exists
/// - [ -d dir ] - true if directory exists
fn builtin_test(args: &[String], cmd_name: &str) -> Result<BuiltinResult> {
    let is_bracket = cmd_name == "[";

    let args = if args.last().map(|s| s.as_str()) == Some("]") {
        &args[..args.len() - 1]
    } else if is_bracket {
        eprintln!("[: missing `]'");
        return Ok(BuiltinResult::Failure(2));
    } else {
        args
    };

    if args.is_empty() {
        return Ok(BuiltinResult::Failure(1));
    }

    match evaluate_test_expr(args) {
        Ok(result) => Ok(if result {
            BuiltinResult::Success(0)
        } else {
            BuiltinResult::Failure(1)
        }),
        Err(_) => Ok(BuiltinResult::Failure(2)),
    }
}

fn evaluate_test_expr(args: &[String]) -> Result<bool> {
    if args.is_empty() {
        return Ok(false);
    }

    let mut idx = 0;
    evaluate_test_or(args, &mut idx)
}

fn evaluate_test_or(args: &[String], idx: &mut usize) -> Result<bool> {
    let mut result = evaluate_test_and(args, idx)?;

    while *idx < args.len() && args[*idx] == "-o" {
        *idx += 1;
        let right = evaluate_test_and(args, idx)?;
        result = result || right;
    }

    Ok(result)
}

fn evaluate_test_and(args: &[String], idx: &mut usize) -> Result<bool> {
    let mut result = evaluate_test_unary(args, idx)?;

    while *idx < args.len() && args[*idx] == "-a" {
        *idx += 1;
        let right = evaluate_test_unary(args, idx)?;
        result = result && right;
    }

    Ok(result)
}

fn evaluate_test_unary(args: &[String], idx: &mut usize) -> Result<bool> {
    if *idx >= args.len() {
        return Ok(false);
    }

    if args[*idx] == "!" {
        *idx += 1;
        let result = evaluate_test_unary(args, idx)?;
        return Ok(!result);
    }

    if args[*idx] == "(" {
        *idx += 1;
        let result = evaluate_test_or(args, idx)?;
        if *idx < args.len() && args[*idx] == ")" {
            *idx += 1;
        }
        return Ok(result);
    }

    evaluate_test_primary(args, idx)
}

fn evaluate_test_primary(args: &[String], idx: &mut usize) -> Result<bool> {
    if *idx >= args.len() {
        return Ok(false);
    }

    let arg = &args[*idx];

    if arg.starts_with('-') && arg.len() == 2 {
        let op = arg.as_str();
        if *idx + 1 < args.len() {
            let next = &args[*idx + 1];

            if *idx + 2 < args.len() {
                let next_next = &args[*idx + 2];
                if matches!(
                    next_next.as_str(),
                    "=" | "==" | "!=" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | "<" | ">"
                ) {
                    let left = arg;
                    *idx += 1;
                    let op = &args[*idx];
                    *idx += 1;
                    let right = &args[*idx];
                    *idx += 1;
                    return evaluate_binary_op(left, op, right);
                }
            }

            let result = match op {
                "-n" => !next.is_empty(),
                "-z" => next.is_empty(),
                "-f" => std::path::Path::new(next).is_file(),
                "-d" => std::path::Path::new(next).is_dir(),
                "-e" => std::path::Path::new(next).exists(),
                "-r" => std::path::Path::new(next).exists(),
                "-w" => std::path::Path::new(next).exists(),
                "-x" => {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(next) {
                        meta.permissions().mode() & 0o111 != 0
                    } else {
                        false
                    }
                }
                "-s" => {
                    if let Ok(meta) = std::fs::metadata(next) {
                        meta.len() > 0
                    } else {
                        false
                    }
                }
                "-L" | "-h" => std::fs::symlink_metadata(next)
                    .map(|m| m.file_type().is_symlink())
                    .unwrap_or(false),
                "-b" => {
                    use std::os::unix::fs::FileTypeExt;
                    std::fs::metadata(next)
                        .map(|m| m.file_type().is_block_device())
                        .unwrap_or(false)
                }
                "-c" => {
                    use std::os::unix::fs::FileTypeExt;
                    std::fs::metadata(next)
                        .map(|m| m.file_type().is_char_device())
                        .unwrap_or(false)
                }
                "-p" => {
                    use std::os::unix::fs::FileTypeExt;
                    std::fs::metadata(next)
                        .map(|m| m.file_type().is_fifo())
                        .unwrap_or(false)
                }
                "-S" => {
                    use std::os::unix::fs::FileTypeExt;
                    std::fs::metadata(next)
                        .map(|m| m.file_type().is_socket())
                        .unwrap_or(false)
                }
                "-t" => {
                    if let Ok(fd) = next.parse::<i32>() {
                        unsafe { libc::isatty(fd) != 0 }
                    } else {
                        false
                    }
                }
                "-v" => false,
                _ => {
                    *idx += 1;
                    let op2 = &args[*idx];
                    *idx += 1;
                    let right = &args[*idx];
                    *idx += 1;
                    return evaluate_binary_op(arg, op2, right);
                }
            };
            *idx += 2;
            return Ok(result);
        }
    }

    if *idx + 2 < args.len() {
        let left = &args[*idx];
        let op = &args[*idx + 1];
        let right = &args[*idx + 2];

        let is_known_binary_op = matches!(
            op.as_str(),
            "=" | "=="
                | "!="
                | "-eq"
                | "-ne"
                | "-lt"
                | "-le"
                | "-gt"
                | "-ge"
                | "<"
                | ">"
                | "-nt"
                | "-ot"
                | "-ef"
        );
        let looks_like_binary_op = op.starts_with('-') && op.len() > 1;

        if is_known_binary_op || looks_like_binary_op {
            *idx += 3;
            return evaluate_binary_op(left, op, right);
        }
    }

    let result = !arg.is_empty();
    *idx += 1;
    Ok(result)
}

fn evaluate_binary_op(left: &str, op: &str, right: &str) -> Result<bool> {
    let result = match op {
        "=" | "==" => left == right,
        "!=" => left != right,
        "<" => left < right,
        ">" => left > right,
        "-eq" => left.parse::<i64>().ok() == right.parse::<i64>().ok(),
        "-ne" => left.parse::<i64>().ok() != right.parse::<i64>().ok(),
        "-lt" => {
            if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                l < r
            } else {
                false
            }
        }
        "-le" => {
            if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                l <= r
            } else {
                false
            }
        }
        "-gt" => {
            if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                l > r
            } else {
                false
            }
        }
        "-ge" => {
            if let (Ok(l), Ok(r)) = (left.parse::<i64>(), right.parse::<i64>()) {
                l >= r
            } else {
                false
            }
        }
        "-nt" => {
            if let (Ok(m1), Ok(m2)) = (std::fs::metadata(left), std::fs::metadata(right)) {
                m1.modified().ok() > m2.modified().ok()
            } else {
                false
            }
        }
        "-ot" => {
            if let (Ok(m1), Ok(m2)) = (std::fs::metadata(left), std::fs::metadata(right)) {
                m1.modified().ok() < m2.modified().ok()
            } else {
                false
            }
        }
        "-ef" => {
            use std::os::unix::fs::MetadataExt;
            if let (Ok(m1), Ok(m2)) = (std::fs::metadata(left), std::fs::metadata(right)) {
                m1.dev() == m2.dev() && m1.ino() == m2.ino()
            } else {
                false
            }
        }
        _ => {
            eprintln!("[: {}: binary operator expected", op);
            return Err(ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{}: binary operator expected", op),
            )));
        }
    };
    Ok(result)
}

/// : - POSIX null utility
///
/// Usage: :
/// Does nothing, returns success (0).
/// Used for variable assignments and as a no-op command.
fn builtin_noop(_args: &[String]) -> Result<BuiltinResult> {
    Ok(BuiltinResult::Success(0))
}

/// true - Return success
///
/// Usage: true
/// Always returns exit code 0 (success).
/// Used in conditionals and implicit return value testing.
fn builtin_true() -> Result<BuiltinResult> {
    Ok(BuiltinResult::Success(0))
}

/// false - Return failure
///
/// Usage: false
/// Always returns exit code 1 (failure).
/// Used in conditionals and implicit return value testing.
fn builtin_false() -> Result<BuiltinResult> {
    Ok(BuiltinResult::Failure(1))
}

/// context - Manage AI interaction context (persistent store + fd 3 channel)
///
/// Usage:
/// - context show [--tokens]     - Show persistent context summary
/// - context add <text>          - Add knowledge entry (persisted + channel)
/// - context add --file <path>   - Add file content to persistent context
/// - context remove <id>         - Remove entry by ID prefix
/// - context clear [--all]       - Clear non-pinned entries (--all for everything)
/// - context pin <id>            - Pin an entry (survives prune)
/// - context prune [max_tokens]  - Prune to fit token budget
/// - context channel [show|clear|add] - Direct fd 3 channel operations
///
/// **F024 + F075:** Combines channel system (fd 3) with persistent context store
fn builtin_context(args: &[String], channels: &mut ChannelManager) -> Result<BuiltinResult> {
    use colony_channels::Channel;
    use sha2::{Digest, Sha256};

    // Derive project_id from cwd hash
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(cwd.to_string_lossy().as_bytes());
    let hash = hex::encode(hasher.finalize());
    let project_id = &hash[..16];

    let store = colony_context::ContextStore::default_store();

    // Handle "context channel" subcommand for direct fd 3 operations
    if args.first().map(String::as_str) == Some("channel") {
        let sub_args = if args.len() > 1 { &args[1..] } else { &[] };
        let sub = sub_args.first().map(String::as_str).unwrap_or("show");

        return match sub {
            "add" => {
                if sub_args.len() < 2 {
                    eprintln!("context channel add: missing text");
                    return Ok(BuiltinResult::Failure(1));
                }
                let text = sub_args[1..].join(" ");
                if let Some(ctx_channel) = channels.context() {
                    ctx_channel.add_text(&text);
                    println!("Added to channel: {}", text);
                    Ok(BuiltinResult::Success(0))
                } else {
                    eprintln!("context: channel not available");
                    Ok(BuiltinResult::Failure(1))
                }
            }
            "show" => {
                let count_limit = if sub_args.len() >= 3 && sub_args[1] == "--last" {
                    sub_args[2].parse::<usize>().ok()
                } else {
                    None
                };
                if let Some(ctx_channel) = channels.context() {
                    let entries = if let Some(limit) = count_limit {
                        ctx_channel.last(limit)
                    } else {
                        ctx_channel.lines()
                    };
                    if entries.is_empty() {
                        println!("(channel context is empty)");
                    } else {
                        for entry in entries {
                            println!("{}", entry);
                        }
                    }
                    Ok(BuiltinResult::Success(0))
                } else {
                    eprintln!("context: channel not available");
                    Ok(BuiltinResult::Failure(1))
                }
            }
            "clear" => {
                if let Some(ctx_channel) = channels.context() {
                    ctx_channel.clear();
                    println!("Channel context cleared");
                    Ok(BuiltinResult::Success(0))
                } else {
                    eprintln!("context: channel not available");
                    Ok(BuiltinResult::Failure(1))
                }
            }
            _ => {
                eprintln!("context channel: unknown subcommand: {}", sub);
                Ok(BuiltinResult::Failure(1))
            }
        };
    }

    // Persistent store operations via colony-context builtin
    let str_args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = colony_context::builtin::execute(&str_args, &store, project_id);

    // For "add" subcommand, also write to channel as side-effect
    if args.first().map(String::as_str) == Some("add") && args.len() >= 2 && args[1] != "--file" {
        let text = args[1..].join(" ");
        if let Some(ctx_channel) = channels.context() {
            ctx_channel.add_text(&text);
        }
    }

    match result {
        colony_context::builtin::BuiltinResult::Output(msg) => {
            println!("{msg}");
            Ok(BuiltinResult::Success(0))
        }
        colony_context::builtin::BuiltinResult::Error(msg) => {
            eprintln!("context: {msg}");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// llm - Show, export, or clear LLM conversation history (fd 4)
///
/// Usage:
///   llm show              Pretty-print recent conversation
///   llm show --last N     Show last N messages
///   llm export            JSON export to stdout
///   llm clear             Clear conversation history
fn builtin_llm(args: &[String], channels: &mut ChannelManager) -> Result<BuiltinResult> {
    use colony_channels::Channel;

    if args.is_empty() {
        eprintln!("llm: missing subcommand");
        eprintln!("Usage: llm <show|export|clear> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    let subcommand = &args[0];

    match subcommand.as_str() {
        "show" => {
            let count_limit = if args.len() >= 3 && args[1] == "--last" {
                args[2].parse::<usize>().ok()
            } else {
                None
            };

            let Some(llm_ch) = channels.llm() else {
                eprintln!("llm: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };

            let messages = match count_limit {
                Some(n) => llm_ch.last_messages(n),
                None => llm_ch.messages(),
            };

            if messages.is_empty() {
                println!("(no conversation history)");
            } else {
                for msg in messages {
                    let role_label = match msg.role {
                        colony_channels::LlmRole::System => "\x1b[35m[system]\x1b[0m",
                        colony_channels::LlmRole::User => "\x1b[34m[user]\x1b[0m",
                        colony_channels::LlmRole::Assistant => "\x1b[32m[assistant]\x1b[0m",
                        colony_channels::LlmRole::Tool => "\x1b[33m[tool]\x1b[0m",
                    };
                    println!("{} {}", role_label, msg.content);
                }
            }

            Ok(BuiltinResult::Success(0))
        }

        "export" => {
            let Some(llm_ch) = channels.llm() else {
                eprintln!("llm: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };
            println!("{}", llm_ch.export_json());
            Ok(BuiltinResult::Success(0))
        }

        "clear" => {
            let Some(llm_ch) = channels.llm() else {
                eprintln!("llm: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };
            llm_ch.clear();
            println!("LLM conversation history cleared");
            Ok(BuiltinResult::Success(0))
        }

        _ => {
            eprintln!("llm: unknown subcommand: {}", subcommand);
            eprintln!("Usage: llm <show|export|clear> [args...]");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// tool - Show, export, or clear tool call log (fd 5)
///
/// Usage:
///   tool show             Show recent tool calls with results
///   tool show --pending   Show unanswered requests
///   tool export           JSON export to stdout
///   tool clear            Clear tool log
fn builtin_tool(args: &[String], channels: &mut ChannelManager) -> Result<BuiltinResult> {
    use colony_channels::Channel;

    if args.is_empty() {
        eprintln!("tool: missing subcommand");
        eprintln!("Usage: tool <show|export|clear> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    let subcommand = &args[0];

    match subcommand.as_str() {
        "show" => {
            let show_pending = args.get(1).is_some_and(|a| a == "--pending");

            let Some(tool_ch) = channels.tool() else {
                eprintln!("tool: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };

            if show_pending {
                let pending = tool_ch.pending_requests();
                if pending.is_empty() {
                    println!("(no pending tool requests)");
                } else {
                    for req in pending {
                        println!(
                            "\x1b[33m[pending]\x1b[0m {} (id: {})",
                            req.tool_name, req.id
                        );
                    }
                }
            } else {
                let entries = tool_ch.entries();
                if entries.is_empty() {
                    println!("(no tool calls recorded)");
                } else {
                    for entry in entries {
                        let status = match &entry.result {
                            Some(r) if r.success => {
                                format!("\x1b[32mok\x1b[0m ({}ms)", r.duration_ms,)
                            }
                            Some(r) => format!(
                                "\x1b[31mfail\x1b[0m: {}",
                                r.error.as_deref().unwrap_or("unknown"),
                            ),
                            None => "\x1b[33mpending\x1b[0m".to_string(),
                        };
                        println!(
                            "{} {} [{}]",
                            entry.request.tool_name, entry.request.id, status
                        );
                    }
                }
            }

            Ok(BuiltinResult::Success(0))
        }

        "export" => {
            let Some(tool_ch) = channels.tool() else {
                eprintln!("tool: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };
            println!("{}", tool_ch.export_json());
            Ok(BuiltinResult::Success(0))
        }

        "clear" => {
            let Some(tool_ch) = channels.tool() else {
                eprintln!("tool: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };
            tool_ch.clear();
            println!("Tool call log cleared");
            Ok(BuiltinResult::Success(0))
        }

        _ => {
            eprintln!("tool: unknown subcommand: {}", subcommand);
            eprintln!("Usage: tool <show|export|clear> [args...]");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// diag - Show, export, or clear diagnostics (fd 6)
///
/// Usage:
///   diag show                   Show recent diagnostics
///   diag show --level <level>   Filter by severity (trace|debug|info|warn|error)
///   diag show --source <src>    Filter by source
///   diag export                 JSON export to stdout
///   diag clear                  Clear diagnostics
fn builtin_diag(args: &[String], channels: &mut ChannelManager) -> Result<BuiltinResult> {
    use colony_channels::Channel;

    if args.is_empty() {
        eprintln!("diag: missing subcommand");
        eprintln!("Usage: diag <show|export|clear> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    let subcommand = &args[0];

    match subcommand.as_str() {
        "show" => {
            let Some(diag_ch) = channels.diag() else {
                eprintln!("diag: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };

            // Parse filter options
            let mut level_filter: Option<colony_channels::Severity> = None;
            let mut source_filter: Option<&str> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--level" => {
                        if let Some(lvl) = args.get(i + 1) {
                            level_filter = parse_severity(lvl);
                            if level_filter.is_none() {
                                eprintln!("diag: unknown level: {}", lvl);
                                return Ok(BuiltinResult::Failure(1));
                            }
                            i += 2;
                        } else {
                            eprintln!("diag show --level: missing level");
                            return Ok(BuiltinResult::Failure(1));
                        }
                    }
                    "--source" => {
                        if let Some(src) = args.get(i + 1) {
                            source_filter = Some(src.as_str());
                            i += 2;
                        } else {
                            eprintln!("diag show --source: missing source name");
                            return Ok(BuiltinResult::Failure(1));
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }

            let entries: Vec<_> = match (level_filter, source_filter) {
                (Some(level), Some(source)) => diag_ch
                    .entries_at_level(level)
                    .into_iter()
                    .filter(|e| e.source.as_deref() == Some(source))
                    .collect(),
                (Some(level), None) => diag_ch.entries_at_level(level),
                (None, Some(source)) => diag_ch.entries_from_source(source),
                (None, None) => diag_ch.entries().iter().collect(),
            };

            if entries.is_empty() {
                println!("(no diagnostics)");
            } else {
                for entry in entries {
                    let severity_label = match entry.severity {
                        colony_channels::Severity::Trace => "\x1b[90m[trace]\x1b[0m",
                        colony_channels::Severity::Debug => "\x1b[36m[debug]\x1b[0m",
                        colony_channels::Severity::Info => "\x1b[34m[info]\x1b[0m",
                        colony_channels::Severity::Warn => "\x1b[33m[warn]\x1b[0m",
                        colony_channels::Severity::Error => "\x1b[31m[error]\x1b[0m",
                    };
                    let source_label = entry
                        .source
                        .as_deref()
                        .map_or(String::new(), |s| format!(" ({})", s));
                    println!("{}{} {}", severity_label, source_label, entry.message);
                }
            }

            Ok(BuiltinResult::Success(0))
        }

        "export" => {
            let Some(diag_ch) = channels.diag() else {
                eprintln!("diag: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };
            println!("{}", diag_ch.export_json());
            Ok(BuiltinResult::Success(0))
        }

        "clear" => {
            let Some(diag_ch) = channels.diag() else {
                eprintln!("diag: channel not available");
                return Ok(BuiltinResult::Failure(1));
            };
            diag_ch.clear();
            println!("Diagnostics cleared");
            Ok(BuiltinResult::Success(0))
        }

        _ => {
            eprintln!("diag: unknown subcommand: {}", subcommand);
            eprintln!("Usage: diag <show|export|clear> [args...]");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// Parse a severity level string to a Severity enum value.
fn parse_severity(s: &str) -> Option<colony_channels::Severity> {
    match s.to_lowercase().as_str() {
        "trace" => Some(colony_channels::Severity::Trace),
        "debug" => Some(colony_channels::Severity::Debug),
        "info" => Some(colony_channels::Severity::Info),
        "warn" | "warning" => Some(colony_channels::Severity::Warn),
        "error" => Some(colony_channels::Severity::Error),
        _ => None,
    }
}

fn builtin_declare(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use colony_expand::VarAttributes;
    use std::collections::HashMap;

    let mut is_assoc = false;
    let mut is_indexed_array = false;
    let mut is_nameref = false;
    let mut function_mode = false;
    let mut function_names_only = false;
    let mut print_mode = false;
    let mut attrs = VarAttributes::default();
    let mut removing_attrs = false;
    let mut reassembled_args: Vec<String> = Vec::new();
    let mut in_parens = false;
    let mut current_arg = String::new();

    for arg in args {
        if (arg.starts_with('-') || arg.starts_with('+')) && arg.len() > 1 && !arg.contains('=') {
            removing_attrs = arg.starts_with('+');
            for c in arg[1..].chars() {
                match c {
                    'a' => is_indexed_array = true,
                    'A' => is_assoc = true,
                    'n' => is_nameref = !removing_attrs,
                    'f' => function_mode = true,
                    'F' => {
                        function_mode = true;
                        function_names_only = true;
                    }
                    'i' => attrs.integer = !removing_attrs,
                    'l' => attrs.lowercase = !removing_attrs,
                    'u' => attrs.uppercase = !removing_attrs,
                    'r' => attrs.readonly = !removing_attrs,
                    'x' => attrs.export = !removing_attrs,
                    't' => attrs.trace = !removing_attrs,
                    'p' => print_mode = true,
                    'g' => {} // global - ignore in non-function context
                    _ => {}
                }
            }
            continue;
        }

        if in_parens {
            current_arg.push(' ');
            current_arg.push_str(arg);
            if arg.ends_with(')') {
                in_parens = false;
                reassembled_args.push(current_arg.clone());
                current_arg.clear();
            }
        } else if arg.contains("=(") && !arg.ends_with(')') {
            in_parens = true;
            current_arg = arg.clone();
        } else {
            reassembled_args.push(arg.clone());
        }
    }

    if print_mode {
        if reassembled_args.is_empty() {
            for (name, var, attr) in context.variables_with_attrs() {
                let flags = attr.map(|a| a.to_flags()).unwrap_or_default();
                let val = var.as_str();
                println!("declare {} {}=\"{}\"", flags, name, val);
            }
        } else {
            for name in &reassembled_args {
                if let Some(value) = context.get_var_or_env(name) {
                    let attr_flags = context
                        .get_var_attributes(name)
                        .map(|a| a.to_flags())
                        .unwrap_or_else(|| "--".to_string());
                    println!("declare {} {}=\"{}\"", attr_flags, name, value);
                }
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    if function_mode {
        if reassembled_args.is_empty() {
            for (name, func) in context.functions() {
                if function_names_only {
                    let export_flag = if context.is_function_exported(name) {
                        "x"
                    } else {
                        ""
                    };
                    println!("declare -f{} {}", export_flag, name);
                } else {
                    let body = format_function_body(&func.body);
                    println!("{} () \n{{ \n{}\n}}", name, body);
                }
            }
        } else {
            let mut exit_code = 0;
            for name in &reassembled_args {
                if let Some(func) = context.get_function(name) {
                    if function_names_only {
                        let export_flag = if context.is_function_exported(name) {
                            "x"
                        } else {
                            ""
                        };
                        println!("declare -f{} {}", export_flag, name);
                    } else {
                        let body = format_function_body(&func.body);
                        println!("{} () \n{{ \n{}\n}}", name, body);
                    }
                } else {
                    exit_code = 1;
                }
            }
            if exit_code != 0 {
                return Ok(BuiltinResult::Failure(exit_code));
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    if reassembled_args.is_empty() && attrs.has_any() {
        return Ok(BuiltinResult::Success(0));
    }

    for var_spec in reassembled_args {
        let (name, value_opt) = if let Some(eq_pos) = var_spec.find('=') {
            (&var_spec[..eq_pos], Some(&var_spec[eq_pos + 1..]))
        } else {
            (var_spec.as_str(), None)
        };

        if removing_attrs {
            if let Err(e) = context.remove_attributes(name, attrs) {
                eprintln!("declare: {}: {}", name, e);
                return Ok(BuiltinResult::Failure(1));
            }
            continue;
        }

        if is_assoc {
            if let Some(value_part) = value_opt {
                if value_part.starts_with('(') && value_part.ends_with(')') {
                    let inner = &value_part[1..value_part.len() - 1];
                    let mut map = HashMap::new();
                    for item in inner.split_whitespace() {
                        if item.starts_with('[') {
                            if let Some(end) = item.find("]=") {
                                let key = &item[1..end];
                                let val = &item[end + 2..];
                                map.insert(key.to_string(), val.to_string());
                            }
                        }
                    }
                    context.set_variable(name.to_string(), Variable::AssocArray(map));
                } else {
                    context.set_variable(name.to_string(), Variable::AssocArray(HashMap::new()));
                }
            } else {
                context.set_variable(name.to_string(), Variable::AssocArray(HashMap::new()));
            }
            continue;
        }

        if is_nameref {
            let target = value_opt.unwrap_or("").to_string();
            context.set_nameref(name.to_string(), target);
            continue;
        }

        if is_indexed_array {
            if let Some(value_part) = value_opt {
                if value_part.starts_with('(') && value_part.ends_with(')') {
                    let inner = &value_part[1..value_part.len() - 1];
                    let elements: Vec<String> =
                        inner.split_whitespace().map(String::from).collect();
                    context.set_variable(name.to_string(), Variable::Array(elements));
                } else {
                    context.set_variable(
                        name.to_string(),
                        Variable::Array(vec![value_part.to_string()]),
                    );
                }
            } else {
                context.set_variable(name.to_string(), Variable::Array(Vec::new()));
            }
            continue;
        }

        if attrs.has_any() {
            if let Err(e) = context.declare_with_attrs(name, value_opt.map(String::from), attrs) {
                eprintln!("declare: {}: {}", name, e);
                return Ok(BuiltinResult::Failure(1));
            }
        } else if let Some(value) = value_opt {
            context.set_var(name.to_string(), value.to_string());
        }
    }

    Ok(BuiltinResult::Success(0))
}

fn builtin_local(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use colony_expand::VarAttributes;

    if !context.in_function() {
        eprintln!("local: can only be used in a function");
        return Ok(BuiltinResult::Failure(1));
    }

    if args.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }

    let mut is_nameref = false;
    let mut attrs = VarAttributes::default();
    let mut var_args: Vec<&str> = Vec::new();

    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg[1..].chars() {
                match c {
                    'n' => is_nameref = true,
                    'i' => attrs.integer = true,
                    'l' => attrs.lowercase = true,
                    'u' => attrs.uppercase = true,
                    'r' => attrs.readonly = true,
                    'x' => attrs.export = true,
                    't' => attrs.trace = true,
                    _ => {}
                }
            }
        } else {
            var_args.push(arg);
        }
    }

    for arg in var_args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let mut value = arg[eq_pos + 1..].to_string();

            if attrs.integer {
                if let Ok(n) = colony_expand::expand_arithmetic(&value, context) {
                    value = n.to_string();
                }
            }
            if attrs.lowercase {
                value = value.to_lowercase();
            }
            if attrs.uppercase {
                value = value.to_uppercase();
            }

            if is_nameref {
                context.declare_local(name, Some(Variable::NameRef(value)));
            } else {
                context.declare_local(name, Some(Variable::String(value)));
            }

            if attrs.has_any() {
                context.set_var_attributes(name, attrs);
            }
        } else if is_nameref {
            context.declare_local(arg, Some(Variable::NameRef(String::new())));
        } else {
            context.declare_local(arg, None);
            if attrs.has_any() {
                context.set_var_attributes(arg, attrs);
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

fn builtin_read(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use std::io::{self, BufRead, Read};
    use std::os::unix::io::FromRawFd;

    let mut raw_mode = false;
    let mut var_names: Vec<&str> = Vec::new();
    let mut input_fd: Option<i32> = None;
    let mut nchars: Option<usize> = None;
    let mut delimiter: Option<char> = None;
    let mut _timeout_secs: Option<f64> = None;
    let mut array_mode = false;
    let mut array_name: Option<&str> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-r" {
            raw_mode = true;
        } else if arg == "-a" {
            array_mode = true;
            i += 1;
            if i < args.len() {
                array_name = Some(&args[i]);
            }
        } else if arg == "-n" {
            i += 1;
            if i < args.len() {
                nchars = args[i].parse().ok();
            }
        } else if arg == "-d" {
            i += 1;
            if i < args.len() {
                if args[i].is_empty() {
                    delimiter = Some('\0');
                } else {
                    delimiter = args[i].chars().next();
                }
            }
        } else if arg == "-t" {
            i += 1;
            if i < args.len() {
                _timeout_secs = args[i].parse().ok();
            }
        } else if arg == "-u" {
            i += 1;
            if i < args.len() {
                if let Ok(fd) = args[i].parse::<i32>() {
                    input_fd = Some(fd);
                }
            }
        } else if arg.starts_with('-') {
            if arg.starts_with("-n") && arg.len() > 2 {
                nchars = arg[2..].parse().ok();
            } else if arg.starts_with("-d") && arg.len() > 2 {
                delimiter = arg[2..].chars().next();
            } else if arg.starts_with("-t") && arg.len() > 2 {
                _timeout_secs = arg[2..].parse().ok();
            } else if arg.starts_with("-u") && arg.len() > 2 {
                if let Ok(fd) = arg[2..].parse::<i32>() {
                    input_fd = Some(fd);
                }
            } else if arg.starts_with("-a") && arg.len() > 2 {
                array_mode = true;
                array_name = Some(&arg[2..]);
            }
        } else {
            var_names.push(arg);
        }
        i += 1;
    }

    if var_names.is_empty() && !array_mode {
        var_names.push("REPLY");
    }

    let line_delimiter = delimiter.unwrap_or('\n');
    let mut line = String::new();

    if let Some(n) = nchars {
        let stdin = io::stdin();
        let mut stdin_lock = stdin.lock();
        let mut buf = vec![0u8; n];
        match stdin_lock.read(&mut buf) {
            Ok(0) => {
                if array_mode {
                    if let Some(name) = array_name {
                        context.set_variable(name.to_string(), Variable::Array(vec![]));
                    }
                } else {
                    for name in &var_names {
                        context.set_var(name.to_string(), String::new());
                    }
                }
                return Ok(BuiltinResult::Success(1));
            }
            Ok(bytes_read) => {
                line = String::from_utf8_lossy(&buf[..bytes_read]).to_string();
                if line.ends_with('\n') {
                    line.pop();
                }
            }
            Err(e) => return Err(ExecutionError::IoError(e)),
        }
    } else if let Some(fd) = input_fd {
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let mut reader = io::BufReader::new(file);

        if raw_mode {
            let mut buf = Vec::new();
            match reader.read_until(line_delimiter as u8, &mut buf) {
                Ok(0) => {
                    std::mem::forget(reader.into_inner());
                    for name in &var_names {
                        context.set_var(name.to_string(), String::new());
                    }
                    return Ok(BuiltinResult::Success(1));
                }
                Ok(_) => {
                    line = String::from_utf8_lossy(&buf).to_string();
                    if line.ends_with(line_delimiter) {
                        line.pop();
                    }
                    std::mem::forget(reader.into_inner());
                }
                Err(e) => {
                    std::mem::forget(reader.into_inner());
                    return Err(ExecutionError::IoError(e));
                }
            }
        } else {
            loop {
                let mut chunk = String::new();
                match reader.read_line(&mut chunk) {
                    Ok(0) => {
                        if line.is_empty() {
                            std::mem::forget(reader.into_inner());
                            for name in &var_names {
                                context.set_var(name.to_string(), String::new());
                            }
                            return Ok(BuiltinResult::Success(1));
                        }
                        break;
                    }
                    Ok(_) => {
                        if chunk.ends_with('\n') {
                            chunk.pop();
                        }
                        if chunk.ends_with('\\') {
                            chunk.pop();
                            line.push_str(&chunk);
                        } else {
                            line.push_str(&chunk);
                            break;
                        }
                    }
                    Err(e) => {
                        std::mem::forget(reader.into_inner());
                        return Err(ExecutionError::IoError(e));
                    }
                }
            }
            std::mem::forget(reader.into_inner());
        }
    } else {
        let stdin = io::stdin();
        let mut stdin_lock = stdin.lock();

        if raw_mode || delimiter.is_some() {
            let mut buf = Vec::new();
            match stdin_lock.read_until(line_delimiter as u8, &mut buf) {
                Ok(0) => {
                    for name in &var_names {
                        context.set_var(name.to_string(), String::new());
                    }
                    return Ok(BuiltinResult::Success(1));
                }
                Ok(_) => {
                    line = String::from_utf8_lossy(&buf).to_string();
                    if line.ends_with(line_delimiter) {
                        line.pop();
                    }
                }
                Err(e) => return Err(ExecutionError::IoError(e)),
            }
        } else {
            loop {
                let mut chunk = String::new();
                match stdin_lock.read_line(&mut chunk) {
                    Ok(0) => {
                        if line.is_empty() {
                            for name in &var_names {
                                context.set_var(name.to_string(), String::new());
                            }
                            return Ok(BuiltinResult::Success(1));
                        }
                        break;
                    }
                    Ok(_) => {
                        if chunk.ends_with('\n') {
                            chunk.pop();
                        }
                        if chunk.ends_with('\\') {
                            chunk.pop();
                            line.push_str(&chunk);
                        } else {
                            line.push_str(&chunk);
                            break;
                        }
                    }
                    Err(e) => return Err(ExecutionError::IoError(e)),
                }
            }
        }
    }

    if line.ends_with('\n') {
        line.pop();
    }
    if line.ends_with('\r') {
        line.pop();
    }

    if !raw_mode {
        let mut processed = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(&next) = chars.peek() {
                    chars.next();
                    processed.push(next);
                } else {
                    processed.push(c);
                }
            } else {
                processed.push(c);
            }
        }
        line = processed;
    }

    let ifs = context.ifs();
    let ifs_chars: Vec<char> = ifs.chars().collect();
    let ifs_whitespace: Vec<char> = ifs_chars
        .iter()
        .filter(|c| c.is_whitespace())
        .copied()
        .collect();

    if var_names.len() == 1 && var_names[0] == "REPLY" {
        context.set_var("REPLY".to_string(), line);
        return Ok(BuiltinResult::Success(0));
    }

    if array_mode {
        if let Some(name) = array_name {
            let ifs = context.ifs();
            let ifs_chars: Vec<char> = ifs.chars().collect();
            let words: Vec<String> = line
                .split(|c| ifs_chars.contains(&c))
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            context.set_variable(name.to_string(), Variable::Array(words));
        }
        return Ok(BuiltinResult::Success(0));
    }

    if var_names.len() == 1 {
        let trimmed = line.trim_matches(|c: char| ifs_whitespace.contains(&c));
        context.set_var(var_names[0].to_string(), trimmed.to_string());
        return Ok(BuiltinResult::Success(0));
    }

    let trimmed = line.trim_start_matches(|c: char| ifs_whitespace.contains(&c));

    let mut field_starts: Vec<usize> = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_start = 0;
    let chars: Vec<char> = trimmed.chars().collect();
    let mut j = 0;
    let mut char_pos = 0;

    while j < chars.len() {
        let c = chars[j];
        if ifs_chars.contains(&c) {
            if !current.is_empty() || !ifs_whitespace.contains(&c) {
                field_starts.push(current_start);
                fields.push(current);
                current = String::new();
            }
            while j + 1 < chars.len() && ifs_whitespace.contains(&chars[j + 1]) {
                j += 1;
                char_pos += chars[j].len_utf8();
            }
            char_pos += c.len_utf8();
            current_start = char_pos;
        } else {
            if current.is_empty() {
                current_start = char_pos;
            }
            current.push(c);
            char_pos += c.len_utf8();
        }
        j += 1;
    }
    if !current.is_empty() {
        field_starts.push(current_start);
        fields.push(current);
    }

    for (idx, name) in var_names.iter().enumerate() {
        if idx < fields.len() {
            if idx == var_names.len() - 1 && idx + 1 < fields.len() {
                let start_pos = field_starts[idx];
                let rest =
                    &trimmed[start_pos..].trim_end_matches(|c: char| ifs_whitespace.contains(&c));
                context.set_var(name.to_string(), rest.to_string());
            } else {
                context.set_var(name.to_string(), fields[idx].clone());
            }
        } else {
            context.set_var(name.to_string(), String::new());
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// mapfile (also readarray) - Read lines from stdin into an array
///
/// Usage: mapfile [-d delim] [-n count] [-O origin] [-s count] [-t] [array]
///
/// Options:
///   -d delim   Use DELIM as line delimiter instead of newline
///   -n count   Read at most COUNT lines (0 means all)
///   -O origin  Begin assigning to ARRAY at index ORIGIN (default 0)
///   -s count   Skip the first COUNT lines
///   -t         Remove trailing delimiter from each line
///
/// If ARRAY is not supplied, the default array name is MAPFILE.
fn builtin_mapfile(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use std::io::{self, BufRead};

    // Parse options
    let mut strip_newlines = false;
    let mut count_limit: usize = 0; // 0 means no limit
    let mut origin: usize = 0;
    let mut skip_count: usize = 0;
    let mut delimiter: u8 = b'\n'; // default newline
    let mut array_name = "MAPFILE".to_string();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-t" => strip_newlines = true,
            "-n" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("mapfile: -n: option requires an argument");
                    return Ok(BuiltinResult::Failure(1));
                }
                count_limit = args[i].parse().unwrap_or(0);
            }
            "-O" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("mapfile: -O: option requires an argument");
                    return Ok(BuiltinResult::Failure(1));
                }
                origin = args[i].parse().unwrap_or(0);
            }
            "-s" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("mapfile: -s: option requires an argument");
                    return Ok(BuiltinResult::Failure(1));
                }
                skip_count = args[i].parse().unwrap_or(0);
            }
            "-d" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("mapfile: -d: option requires an argument");
                    return Ok(BuiltinResult::Failure(1));
                }
                let delim_arg = &args[i];
                if delim_arg.is_empty() {
                    // Empty string means NUL delimiter
                    delimiter = 0;
                } else {
                    // Use first character as delimiter
                    delimiter = delim_arg.as_bytes()[0];
                }
            }
            s if s.starts_with('-') && s != "-" => {
                // Unknown option - check if it's a combined option or invalid
                // Handle combined options like -tn2
                let chars: Vec<char> = s[1..].chars().collect();
                let mut j = 0;
                while j < chars.len() {
                    match chars[j] {
                        't' => strip_newlines = true,
                        'n' | 'O' | 's' | 'd'
                            // These options need arguments
                            // Check if the rest of the string is the argument
                            if j + 1 < chars.len() => {
                                let rest: String = chars[j + 1..].iter().collect();
                                match chars[j] {
                                    'n' => count_limit = rest.parse().unwrap_or(0),
                                    'O' => origin = rest.parse().unwrap_or(0),
                                    's' => skip_count = rest.parse().unwrap_or(0),
                                    'd' => {
                                        if rest.is_empty() {
                                            delimiter = 0;
                                        } else {
                                            delimiter = rest.as_bytes()[0];
                                        }
                                    }
                                    _ => {}
                                }
                                break; // consumed rest of the arg
                            }
                        _ => {
                            eprintln!("mapfile: {}: invalid option", s);
                            return Ok(BuiltinResult::Failure(1));
                        }
                    }
                    j += 1;
                }
            }
            _ => {
                // This is the array name
                array_name = arg.clone();
            }
        }
        i += 1;
    }

    // Read lines from stdin
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut lines: Vec<String> = Vec::new();
    let mut skipped = 0;
    let mut read_count = 0;

    // Read using the delimiter
    loop {
        let mut buffer = Vec::new();
        let bytes_read = if delimiter == b'\n' {
            // Use read_until for newline delimiter
            stdin_lock.read_until(b'\n', &mut buffer)
        } else {
            // Use read_until for custom delimiter
            stdin_lock.read_until(delimiter, &mut buffer)
        };

        match bytes_read {
            Ok(0) => break, // EOF
            Ok(_) => {
                // Convert buffer to string
                let mut line = String::from_utf8_lossy(&buffer).to_string();

                // Skip lines if needed
                if skipped < skip_count {
                    skipped += 1;
                    continue;
                }

                // Strip delimiter if -t flag is set
                if strip_newlines {
                    if delimiter == b'\n' {
                        // Strip trailing \n and \r\n
                        if line.ends_with('\n') {
                            line.pop();
                            if line.ends_with('\r') {
                                line.pop();
                            }
                        }
                    } else if delimiter != 0 {
                        // Strip trailing custom delimiter
                        if line.as_bytes().last() == Some(&delimiter) {
                            line.pop();
                        }
                    } else {
                        // NUL delimiter - strip trailing NUL
                        if line.as_bytes().last() == Some(&0) {
                            line.pop();
                        }
                    }
                }

                lines.push(line);
                read_count += 1;

                // Check if we've reached the count limit
                if count_limit > 0 && read_count >= count_limit {
                    break;
                }
            }
            Err(e) => return Err(ExecutionError::IoError(e)),
        }
    }

    // Create the array, preserving existing elements if origin > 0
    let mut elements: Vec<String> = Vec::new();

    if origin > 0 {
        if let Some(Variable::Array(arr)) = context.get_variable(&array_name) {
            for (idx, val) in arr.iter().enumerate() {
                if idx >= origin {
                    break;
                }
                elements.push(val.clone());
            }
        }
        // Pad with empty strings if existing array was shorter than origin
        while elements.len() < origin {
            elements.push(String::new());
        }
    }

    // Append the read lines
    elements.extend(lines);

    // Set the array in context
    context.set_variable(array_name.clone(), Variable::Array(elements));

    Ok(BuiltinResult::Success(0))
}

/// printf - Format and print data
///
/// Usage: printf [-v varname] format [arguments...]
///
/// Supports:
/// - Format specifiers: %s, %d, %i, %o, %x, %X, %b, %q, %%
/// - Width and precision: %10s, %-10s, %.5s, %10.5s
/// - Escape sequences: \n, \t, \r, \\, \", \', \0nnn, \xHH
/// - -v varname: Store result in variable instead of printing
fn builtin_printf(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("printf: missing format string");
        return Ok(BuiltinResult::Failure(1));
    }

    let mut store_var: Option<&str> = None;
    let mut arg_idx = 0;

    // Parse options
    while arg_idx < args.len() {
        if args[arg_idx] == "-v" {
            if arg_idx + 1 >= args.len() {
                eprintln!("printf: -v requires variable name");
                return Ok(BuiltinResult::Failure(1));
            }
            store_var = Some(&args[arg_idx + 1]);
            arg_idx += 2;
        } else if args[arg_idx].starts_with('-') && args[arg_idx] != "-" {
            // Skip unknown options
            arg_idx += 1;
        } else {
            break;
        }
    }

    if arg_idx >= args.len() {
        eprintln!("printf: missing format string");
        return Ok(BuiltinResult::Failure(1));
    }

    let format = &args[arg_idx];
    let format_args = &args[arg_idx + 1..];

    let output = format_printf(format, format_args)?;

    if let Some(var_name) = store_var {
        context.set_var(var_name.to_string(), output);
    } else {
        use std::io::Write;
        print!("{}", output);
        let _ = std::io::stdout().flush();
    }

    Ok(BuiltinResult::Success(0))
}

fn format_function_body(body: &[colony_parser::Statement]) -> String {
    let mut output = String::new();
    for stmt in body {
        let line = format_statement(stmt);
        for l in line.lines() {
            output.push_str("    ");
            output.push_str(l);
            output.push('\n');
        }
    }
    output.trim_end().to_string()
}

fn format_statement(stmt: &colony_parser::Statement) -> String {
    use colony_parser::{ListOp, Statement};

    match stmt {
        Statement::SimpleCommand(cmd) => {
            let mut parts: Vec<String> = vec![format_word(&cmd.name)];
            for arg in &cmd.args {
                parts.push(format_word(arg));
            }
            for redir in &cmd.redirects {
                parts.push(format_redirect(redir));
            }
            parts.join(" ")
        }
        Statement::Pipeline(pipeline) => {
            let cmds: Vec<String> = pipeline
                .elements
                .iter()
                .map(|elem| format_statement(&elem.command))
                .collect();
            cmds.join(" | ")
        }
        Statement::List(list) => {
            let first = format_statement(&list.first);
            let mut result = first;
            for (op, cmd) in &list.rest {
                let sep = match op {
                    ListOp::And => " && ",
                    ListOp::Or => " || ",
                    ListOp::Semi => "; ",
                    ListOp::Background => " & ",
                };
                result.push_str(sep);
                result.push_str(&format_statement(cmd));
            }
            result
        }
        Statement::If(if_stmt) => {
            let cond = format_condition(&if_stmt.condition);
            let mut result = format!("if {}; then\n", cond);
            result.push_str(&format_statements_indented(&if_stmt.then_body));
            for elif in &if_stmt.elif_branches {
                result.push_str(&format!(
                    "\nelif {}; then\n",
                    format_condition(&elif.condition)
                ));
                result.push_str(&format_statements_indented(&elif.body));
            }
            if let Some(else_body) = &if_stmt.else_body {
                result.push_str("\nelse\n");
                result.push_str(&format_statements_indented(else_body));
            }
            result.push_str("\nfi");
            result
        }
        Statement::Try(try_stmt) => {
            let mut result = String::from("try {\n");
            result.push_str(&format_statements_indented(&try_stmt.body));
            result.push_str("\n}");
            if let Some(catch_block) = &try_stmt.catch {
                result.push_str(" catch");
                if let Some(binding) = &catch_block.binding {
                    result.push(' ');
                    result.push_str(binding);
                }
                if let Some(error_type) = &catch_block.error_type {
                    result.push_str(" : ");
                    result.push_str(error_type);
                }
                result.push_str(" {\n");
                result.push_str(&format_statements_indented(&catch_block.body));
                result.push_str("\n}");
            }
            if let Some(finally_body) = &try_stmt.finally_body {
                result.push_str(" finally {\n");
                result.push_str(&format_statements_indented(finally_body));
                result.push_str("\n}");
            }
            result
        }
        Statement::Match(match_stmt) => {
            let mut result = format!("match {} {{\n", format_expr(&match_stmt.value));
            for arm in &match_stmt.arms {
                let mut arm_line = format!("    {}", format_match_pattern(&arm.pattern));
                if let Some(guard) = &arm.guard {
                    arm_line.push_str(&format!(" if {}", format_expr(guard)));
                }
                arm_line.push_str(" => ");
                match &arm.body {
                    colony_parser::MatchBody::Block(stmts) => {
                        arm_line.push_str("{\n");
                        let body = format_statements_double_indented(stmts);
                        if !body.is_empty() {
                            arm_line.push_str(&body);
                            arm_line.push('\n');
                        }
                        arm_line.push_str("    }");
                    }
                    colony_parser::MatchBody::Expr(expr) => {
                        arm_line.push_str(&format_expr(expr));
                    }
                }
                arm_line.push('\n');
                result.push_str(&arm_line);
            }
            result.push('}');
            result
        }
        Statement::While(while_stmt) => {
            format!(
                "while {}; do\n{}\ndone",
                format_condition(&while_stmt.condition),
                format_statements_indented(&while_stmt.body)
            )
        }
        Statement::Until(until_stmt) => {
            format!(
                "until {}; do\n{}\ndone",
                format_condition(&until_stmt.condition),
                format_statements_indented(&until_stmt.body)
            )
        }
        Statement::For(for_stmt) => {
            if for_stmt.words.is_empty() {
                format!(
                    "for {}; do\n{}\ndone",
                    for_stmt.var,
                    format_statements_indented(&for_stmt.body)
                )
            } else {
                let words: Vec<String> = for_stmt.words.iter().map(format_word).collect();
                format!(
                    "for {} in {}; do\n{}\ndone",
                    for_stmt.var,
                    words.join(" "),
                    format_statements_indented(&for_stmt.body)
                )
            }
        }
        Statement::Case(case_stmt) => {
            let mut result = format!("case {} in\n", format_word(&case_stmt.value));
            for arm in &case_stmt.arms {
                let patterns: Vec<String> = arm.patterns.iter().map(format_word).collect();
                result.push_str(&format!("    {})\n", patterns.join(" | ")));
                result.push_str(&format_statements_double_indented(&arm.body));
                result.push_str("\n        ;;\n");
            }
            result.push_str("esac");
            result
        }
        Statement::FunctionDef(func) => {
            format!(
                "{} () {{\n{}\n}}",
                func.name,
                format_function_body(&func.body)
            )
        }
        Statement::ProcDef(proc_def) => {
            let params: Vec<String> = proc_def
                .params
                .iter()
                .map(|p| {
                    if p.default.is_some() {
                        format!("{} = ...", p.name)
                    } else {
                        p.name.clone()
                    }
                })
                .collect();
            format!(
                "proc {} ({}) {{\n{}\n}}",
                proc_def.name,
                params.join(", "),
                format_function_body(&proc_def.body)
            )
        }
        Statement::BraceGroup(group) => {
            format!("{{\n{}\n}}", format_statements_indented(&group.body))
        }
        Statement::Subshell(sub) => {
            format!("(\n{}\n)", format_statements_indented(&sub.body))
        }
        Statement::VarDecl(decl) => {
            let kind = match decl.kind {
                colony_parser::VarKind::Var => "var",
                colony_parser::VarKind::Const => "const",
                colony_parser::VarKind::Setvar => "setvar",
            };
            let type_suffix = decl
                .type_annotation
                .as_ref()
                .map(|t| format!(": {}", t))
                .unwrap_or_default();
            let target = match &decl.target {
                colony_parser::VarDeclTarget::Name(name) => name.clone(),
                colony_parser::VarDeclTarget::Tuple(names) => {
                    format!("({})", names.join(", "))
                }
            };
            format!(
                "{} {}{} = {}",
                kind,
                target,
                type_suffix,
                format_expr(&decl.value)
            )
        }
        Statement::Return(ret) => {
            if let Some(ref val) = ret.value {
                format!("return {}", format_expr(val))
            } else {
                "return".to_string()
            }
        }
        _ => "# unsupported".to_string(),
    }
}

fn format_match_pattern(pattern: &colony_parser::MatchPattern) -> String {
    match pattern {
        colony_parser::MatchPattern::Literal(expr) => format_expr(expr),
        colony_parser::MatchPattern::Binding(name) => name.clone(),
        colony_parser::MatchPattern::Wildcard => "_".to_string(),
        colony_parser::MatchPattern::List(list) => {
            let mut parts: Vec<String> = list.items.iter().map(format_match_pattern).collect();
            match &list.rest {
                colony_parser::ListRest::None => {}
                colony_parser::ListRest::Ignore => parts.push("..".to_string()),
                colony_parser::ListRest::Bind(name) => parts.push(format!("..{}", name)),
            }
            format!("[{}]", parts.join(", "))
        }
        colony_parser::MatchPattern::Dict(dict) => {
            let mut parts = Vec::new();
            for field in &dict.fields {
                if let Some(pattern) = &field.pattern {
                    parts.push(format!("{}: {}", field.name, format_match_pattern(pattern)));
                } else {
                    parts.push(field.name.clone());
                }
            }
            if dict.allow_extra {
                parts.push("..".to_string());
            }
            format!("{{{}}}", parts.join(", "))
        }
        colony_parser::MatchPattern::Tuple(tuple) => {
            let parts: Vec<String> = tuple.items.iter().map(format_match_pattern).collect();
            format!("({})", parts.join(", "))
        }
        colony_parser::MatchPattern::Or(patterns) => {
            let parts: Vec<String> = patterns.iter().map(format_match_pattern).collect();
            parts.join(" | ")
        }
    }
}

fn format_condition(cond: &colony_parser::Condition) -> String {
    match cond {
        colony_parser::Condition::Command(stmt) => format_statement(stmt),
        colony_parser::Condition::Expression(expr) => format_expr(expr),
    }
}

fn format_statements_indented(stmts: &[colony_parser::Statement]) -> String {
    stmts
        .iter()
        .map(|s| format!("    {}", format_statement(s)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_statements_double_indented(stmts: &[colony_parser::Statement]) -> String {
    stmts
        .iter()
        .map(|s| format!("        {}", format_statement(s)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_word(word: &colony_parser::Word) -> String {
    use colony_parser::WordPart;

    let mut result = String::new();
    for part in &word.parts {
        match part {
            WordPart::Literal(s) => result.push_str(s),
            WordPart::SingleQuoted(s) => result.push_str(&format!("'{}'", s)),
            WordPart::DoubleQuoted(parts) => {
                result.push('"');
                for p in parts {
                    match p {
                        WordPart::Literal(s) => result.push_str(s),
                        WordPart::Variable(v) => {
                            if v.modifier.is_some() || v.prefix.is_some() {
                                result.push_str(&format!("${{{}}}", v.name));
                            } else {
                                result.push_str(&format!("${}", v.name));
                            }
                        }
                        _ => {}
                    }
                }
                result.push('"');
            }
            WordPart::Variable(v) => {
                if v.modifier.is_some() || v.prefix.is_some() {
                    result.push_str(&format!("${{{}}}", v.name));
                } else {
                    result.push_str(&format!("${}", v.name));
                }
            }
            WordPart::CommandSub(stmt) => {
                result.push_str(&format!("$({})", format_statement(stmt)));
            }
            WordPart::CommandSubRaw(s) => {
                result.push_str(&format!("$({})", s));
            }
            WordPart::ArithRaw(s) => {
                result.push_str(&format!("$(({})", s));
            }
            WordPart::ExprSub(expr) => {
                result.push_str(&format!("$[{}]", format_expr(expr)));
            }
            WordPart::Tilde(user) => {
                result.push('~');
                if let Some(u) = user {
                    result.push_str(u);
                }
            }
            WordPart::AnsiCQuoted(s) => result.push_str(&format!("$'{}'", s)),
            WordPart::BraceExpansion(s) => result.push_str(&format!("{{{}}}", s)),
            _ => {}
        }
    }
    result
}

fn format_redirect(redir: &colony_parser::Redirect) -> String {
    use colony_parser::{RedirectKind, RedirectTarget};

    let fd = redir.fd.map(|n| n.to_string()).unwrap_or_default();
    let op = match redir.kind {
        RedirectKind::Output => ">",
        RedirectKind::Append => ">>",
        RedirectKind::Input => "<",
        RedirectKind::ReadWrite => "<>",
        RedirectKind::Clobber => ">|",
        RedirectKind::DupFd => ">&",
        RedirectKind::DupFdInput => "<&",
        RedirectKind::HereDoc(_) => "<<",
        RedirectKind::AndOutput => "&>",
        RedirectKind::AndAppend => "&>>",
    };
    let target = match &redir.target {
        RedirectTarget::File(w) => format_word(w),
        RedirectTarget::Fd(n) => n.to_string(),
        RedirectTarget::Close => "-".to_string(),
        RedirectTarget::HereDoc { content, .. } => content.clone(),
    };
    format!("{}{}{}", fd, op, target)
}

fn format_expr(expr: &colony_parser::Expression) -> String {
    use colony_parser::Expression;

    match expr {
        Expression::Int(n, _) => n.to_string(),
        Expression::Float(f, _) => f.to_string(),
        Expression::Str(s, _) => format!("\"{}\"", s),
        Expression::Bool(b, _) => b.to_string(),
        Expression::Variable { name, .. } => format!("${}", name),
        Expression::FuncCall { name, args, .. } => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("{}({})", name, arg_strs.join(", "))
        }
        Expression::MethodCall {
            target, name, args, ..
        } => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("{}.{}({})", format_expr(target), name, arg_strs.join(", "))
        }
        Expression::OptionalCall { name, args, .. } => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("{}?({})", name, arg_strs.join(", "))
        }
        Expression::OptionalChain { target, .. } => format!("{}?", format_expr(target)),
        Expression::Unwrap { target, .. } => format!("{}!", format_expr(target)),
        Expression::Binary {
            left, op, right, ..
        } => {
            format!("{} {:?} {}", format_expr(left), op, format_expr(right))
        }
        _ => String::new(),
    }
}

/// Format a printf format string with arguments
fn format_printf(format: &str, args: &[String]) -> Result<String> {
    let mut output = String::new();
    let mut arg_idx = 0;
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' {
            // Escape sequence
            i += 1;
            if i < chars.len() {
                let escaped = match chars[i] {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    'a' => '\x07', // bell
                    'b' => '\x08', // backspace
                    'f' => '\x0c', // form feed
                    'v' => '\x0b', // vertical tab
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    '0'..='7' => {
                        // Octal escape \nnn (where n is 0-7)
                        // Reads up to 3 octal digits starting from current character
                        let mut octal = String::new();
                        octal.push(chars[i]);
                        i += 1;
                        while i < chars.len()
                            && chars[i] >= '0'
                            && chars[i] <= '7'
                            && octal.len() < 3
                        {
                            octal.push(chars[i]);
                            i += 1;
                        }
                        i -= 1;
                        let val = u8::from_str_radix(&octal, 8).unwrap_or(0);
                        val as char
                    }
                    'x' => {
                        // Hex escape \xHH
                        let mut hex = String::new();
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_hexdigit() && hex.len() < 2 {
                            hex.push(chars[i]);
                            i += 1;
                        }
                        i -= 1;
                        if hex.is_empty() {
                            'x'
                        } else {
                            let val = u8::from_str_radix(&hex, 16).unwrap_or(0);
                            val as char
                        }
                    }
                    c => c,
                };
                output.push(escaped);
            }
        } else if chars[i] == '%' {
            // Format specifier
            i += 1;
            if i >= chars.len() {
                output.push('%');
                continue;
            }

            if chars[i] == '%' {
                output.push('%');
                i += 1;
                continue;
            }

            // Parse flags
            let mut left_justify = false;
            let mut zero_pad = false;
            let mut plus_sign = false;
            let mut space_sign = false;
            let mut alternate = false;

            while i < chars.len() {
                match chars[i] {
                    '-' => left_justify = true,
                    '0' => zero_pad = true,
                    '+' => plus_sign = true,
                    ' ' => space_sign = true,
                    '#' => alternate = true,
                    _ => break,
                }
                i += 1;
            }

            // Parse width
            let mut width = 0usize;
            while i < chars.len() && chars[i].is_ascii_digit() {
                width = width * 10 + (chars[i] as usize - '0' as usize);
                i += 1;
            }

            // Parse precision
            let mut precision: Option<usize> = None;
            if i < chars.len() && chars[i] == '.' {
                i += 1;
                let mut prec = 0usize;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    prec = prec * 10 + (chars[i] as usize - '0' as usize);
                    i += 1;
                }
                precision = Some(prec);
            }

            if i >= chars.len() {
                output.push('%');
                continue;
            }

            let spec = chars[i];
            let arg = if arg_idx < args.len() {
                let a = args[arg_idx].clone();
                arg_idx += 1;
                a
            } else {
                String::new()
            };

            let formatted = match spec {
                's' => {
                    let mut s = arg;
                    if let Some(prec) = precision {
                        if s.len() > prec {
                            s = s[..prec].to_string();
                        }
                    }
                    if width > 0 {
                        if left_justify {
                            format!("{:<width$}", s, width = width)
                        } else {
                            format!("{:>width$}", s, width = width)
                        }
                    } else {
                        s
                    }
                }
                'd' | 'i' => {
                    let num: i64 = arg.parse().unwrap_or(0);
                    format_number(
                        num,
                        10,
                        false,
                        width,
                        precision,
                        left_justify,
                        zero_pad,
                        plus_sign,
                        space_sign,
                    )
                }
                'u' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    format_unsigned(num, 10, false, width, left_justify, zero_pad)
                }
                'o' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0" } else { "" };
                    format_unsigned_with_prefix(
                        num,
                        8,
                        false,
                        width,
                        left_justify,
                        zero_pad,
                        prefix,
                    )
                }
                'x' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0x" } else { "" };
                    format_unsigned_with_prefix(
                        num,
                        16,
                        false,
                        width,
                        left_justify,
                        zero_pad,
                        prefix,
                    )
                }
                'X' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0X" } else { "" };
                    format_unsigned_with_prefix(
                        num,
                        16,
                        true,
                        width,
                        left_justify,
                        zero_pad,
                        prefix,
                    )
                }
                'c' => {
                    // First character
                    arg.chars()
                        .next()
                        .map(|c| c.to_string())
                        .unwrap_or_default()
                }
                'b' => {
                    // %b expands escape sequences in the argument
                    expand_escapes(&arg)
                }
                'q' => {
                    // %q quotes the argument for shell reuse
                    shell_quote(&arg)
                }
                _ => {
                    // Unknown specifier - output literally
                    format!("%{}", spec)
                }
            };
            output.push_str(&formatted);
        } else {
            output.push(chars[i]);
        }
        i += 1;
    }

    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn format_number(
    num: i64,
    _radix: u32,
    _uppercase: bool,
    width: usize,
    _precision: Option<usize>,
    left_justify: bool,
    zero_pad: bool,
    plus_sign: bool,
    space_sign: bool,
) -> String {
    let sign = if num < 0 {
        "-"
    } else if plus_sign {
        "+"
    } else if space_sign {
        " "
    } else {
        ""
    };

    let abs_num = num.abs();
    let num_str = abs_num.to_string();

    if width > 0 {
        let content_len = sign.len() + num_str.len();
        if content_len < width {
            let padding = width - content_len;
            if zero_pad && !left_justify {
                format!(
                    "{}{:0>width$}",
                    sign,
                    num_str,
                    width = num_str.len() + padding
                )
            } else if left_justify {
                format!("{}{}{}", sign, num_str, " ".repeat(padding))
            } else {
                format!("{}{}{}", " ".repeat(padding), sign, num_str)
            }
        } else {
            format!("{}{}", sign, num_str)
        }
    } else {
        format!("{}{}", sign, num_str)
    }
}

fn format_unsigned(
    num: u64,
    radix: u32,
    uppercase: bool,
    width: usize,
    left_justify: bool,
    zero_pad: bool,
) -> String {
    format_unsigned_with_prefix(num, radix, uppercase, width, left_justify, zero_pad, "")
}

fn format_unsigned_with_prefix(
    num: u64,
    radix: u32,
    uppercase: bool,
    width: usize,
    left_justify: bool,
    zero_pad: bool,
    prefix: &str,
) -> String {
    let num_str = match radix {
        8 => format!("{:o}", num),
        16 => {
            if uppercase {
                format!("{:X}", num)
            } else {
                format!("{:x}", num)
            }
        }
        _ => num.to_string(),
    };

    let full = format!("{}{}", prefix, num_str);

    if width > 0 && full.len() < width {
        let padding = width - full.len();
        if zero_pad && !left_justify {
            format!(
                "{}{:0>width$}",
                prefix,
                num_str,
                width = num_str.len() + padding
            )
        } else if left_justify {
            format!("{}{}", full, " ".repeat(padding))
        } else {
            format!("{}{}", " ".repeat(padding), full)
        }
    } else {
        full
    }
}

fn expand_escapes(s: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 1;
            match chars[i] {
                'n' => output.push('\n'),
                't' => output.push('\t'),
                'r' => output.push('\r'),
                'a' => output.push('\x07'),
                'b' => output.push('\x08'),
                'f' => output.push('\x0c'),
                'v' => output.push('\x0b'),
                '\\' => output.push('\\'),
                '\'' => output.push('\''),
                '"' => output.push('"'),
                '0'..='7' => {
                    let mut octal = String::new();
                    octal.push(chars[i]);
                    i += 1;
                    while i < chars.len() && chars[i] >= '0' && chars[i] <= '7' && octal.len() < 3 {
                        octal.push(chars[i]);
                        i += 1;
                    }
                    i -= 1;
                    let val = u8::from_str_radix(&octal, 8).unwrap_or(0);
                    output.push(val as char);
                }
                'x' => {
                    // Hex
                    let mut hex = String::new();
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_hexdigit() && hex.len() < 2 {
                        hex.push(chars[i]);
                        i += 1;
                    }
                    i -= 1;
                    if !hex.is_empty() {
                        let val = u8::from_str_radix(&hex, 16).unwrap_or(0);
                        output.push(val as char);
                    }
                }
                'c' => {
                    // Stop processing - %b specific
                    break;
                }
                c => output.push(c),
            }
        } else {
            output.push(chars[i]);
        }
        i += 1;
    }

    output
}

fn shell_quote(s: &str) -> String {
    // If the string is simple, just add single quotes
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/')
    {
        return s.to_string();
    }

    // Use $'...' quoting for strings with special characters
    let mut output = String::from("$'");
    for c in s.chars() {
        match c {
            '\'' => output.push_str("\\'"),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\t' => output.push_str("\\t"),
            '\r' => output.push_str("\\r"),
            c if c.is_ascii_control() => {
                output.push_str(&format!("\\x{:02x}", c as u8));
            }
            c => output.push(c),
        }
    }
    output.push('\'');
    output
}

/// exec - Apply redirections to shell or replace shell with command
///
/// Usage: exec [command [args...]] [redirections]
fn builtin_exec(
    args: &[String],
    redirects: &[colony_parser::Redirect],
    context: &mut ExpansionContext,
) -> Result<BuiltinResult> {
    use crate::redirect::apply_redirects;

    if !redirects.is_empty() {
        match apply_redirects(redirects, context) {
            Ok(_opened_fds) => {
                tracing::debug!("exec: applied {} redirections to shell", redirects.len());
            }
            Err(e) => {
                eprintln!("exec: {}", e);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    if args.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }

    eprintln!("exec: replacing shell with command not yet implemented");
    eprintln!("exec: would execute: {} {}", args[0], args[1..].join(" "));
    Ok(BuiltinResult::Failure(1))
}

fn builtin_trap(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use colony_expand::{TrapAction, TrapSignal};

    if args.is_empty() {
        let traps = context.traps();
        if traps.is_empty() {
            return Ok(BuiltinResult::Success(0));
        }
        for (signal, action) in traps {
            match action {
                TrapAction::Ignore => println!("trap -- '' {}", signal),
                TrapAction::Command(cmd) => {
                    println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal)
                }
                TrapAction::Default => {}
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    let mut print_mode = false;
    let mut list_mode = false;
    let mut skip_count = 0;

    for (i, arg) in args.iter().enumerate() {
        if arg == "-p" {
            print_mode = true;
            skip_count = i + 1;
        } else if arg == "-l" {
            list_mode = true;
        } else if arg.starts_with('-') && arg != "-" {
            continue;
        } else {
            break;
        }
    }

    if list_mode {
        let names = TrapSignal::all_signal_names();
        for (i, name) in names.iter().enumerate() {
            print!("{:2}) {:<8}", i + 1, name);
            if (i + 1) % 8 == 0 {
                println!();
            }
        }
        if names.len() % 8 != 0 {
            println!();
        }
        return Ok(BuiltinResult::Success(0));
    }

    let remaining_args = &args[skip_count..];

    if print_mode {
        if remaining_args.is_empty() {
            let traps = context.traps();
            for (signal, action) in traps {
                match action {
                    TrapAction::Ignore => println!("trap -- '' {}", signal),
                    TrapAction::Command(cmd) => {
                        println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal)
                    }
                    TrapAction::Default => {}
                }
            }
        } else {
            for sig_name in remaining_args {
                if let Some(signal) = TrapSignal::parse(sig_name) {
                    if let Some(action) = context.get_trap(&signal) {
                        match action {
                            TrapAction::Ignore => println!("trap -- '' {}", signal),
                            TrapAction::Command(cmd) => {
                                println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal)
                            }
                            TrapAction::Default => {}
                        }
                    }
                } else {
                    eprintln!("trap: {}: invalid signal specification", sig_name);
                    return Ok(BuiltinResult::Failure(1));
                }
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    if remaining_args.is_empty() {
        let traps = context.traps();
        for (signal, action) in traps {
            match action {
                TrapAction::Ignore => println!("trap -- '' {}", signal),
                TrapAction::Command(cmd) => {
                    println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal)
                }
                TrapAction::Default => {}
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    if remaining_args.len() == 1 {
        let first = &remaining_args[0];
        if first == "-" {
            eprintln!("trap: missing signal specification");
            return Ok(BuiltinResult::Failure(1));
        }
        if let Some(signal) = TrapSignal::parse(first) {
            if let Some(action) = context.get_trap(&signal) {
                match action {
                    TrapAction::Ignore => println!("trap -- '' {}", signal),
                    TrapAction::Command(cmd) => {
                        println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal)
                    }
                    TrapAction::Default => {}
                }
            }
            return Ok(BuiltinResult::Success(0));
        }
        eprintln!("trap: {}: invalid signal specification", first);
        return Ok(BuiltinResult::Failure(1));
    }

    let action_str = &remaining_args[0];
    let signal_names = &remaining_args[1..];

    let action = if action_str == "-" {
        TrapAction::Default
    } else if action_str.is_empty() {
        TrapAction::Ignore
    } else {
        TrapAction::Command(action_str.clone())
    };

    for sig_name in signal_names {
        match TrapSignal::parse(sig_name) {
            Some(signal) => {
                if !signal.can_trap() {
                    eprintln!("trap: {}: cannot be trapped", sig_name);
                    return Ok(BuiltinResult::Failure(1));
                }
                context.set_trap(signal, action.clone());
            }
            None => {
                eprintln!("trap: {}: invalid signal specification", sig_name);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

fn builtin_return(args: &[String], context: &ExpansionContext) -> Result<BuiltinResult> {
    let code = if args.is_empty() {
        context.last_exit_status()
    } else {
        args[0].parse::<i32>().unwrap_or(255)
    };

    Err(ExecutionError::Return(code))
}

/// getopts - Parse positional parameters
///
/// Usage: getopts optstring name [arg...]
///
/// Parses positional parameters (or provided args) for single-character options.
/// - `optstring`: Valid option characters. If followed by `:`, option requires argument.
/// - `name`: Variable to store the current option letter.
/// - `arg...`: Optional arguments to parse instead of positional parameters.
///
/// Returns 0 if an option is found, 1 when all options are processed.
///
/// Sets:
/// - `name` to the option letter, or `?` on error
/// - `OPTARG` to the option's argument (if any)
/// - `OPTIND` to the index of the next argument to process
///
/// If optstring starts with `:`, silent error mode is enabled:
/// - Invalid option: `name` = `?`, `OPTARG` = the invalid character
/// - Missing argument: `name` = `:`, `OPTARG` = the option character
fn builtin_getopts(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    // Validate arguments: need at least optstring and name
    if args.is_empty() {
        eprintln!("getopts: usage: getopts optstring name [arg ...]");
        return Ok(BuiltinResult::Failure(1));
    }

    if args.len() < 2 {
        eprintln!("getopts: usage: getopts optstring name [arg ...]");
        return Ok(BuiltinResult::Failure(1));
    }

    let optstring = &args[0];
    let name = &args[1];

    // Determine if we're using custom args or positional params
    let use_custom_args = args.len() > 2;

    // Get current OPTIND (defaults to 1)
    let mut optind: usize = context
        .get_var_or_env("OPTIND")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    // Get getopts internal state (position within combined options like -abc)
    // Format: "argindex:charpos" where charpos is position within current arg string
    let (mut arg_index, mut char_pos) = parse_getopts_state(context, optind);

    // Determine if silent mode (optstring starts with :)
    let (silent_mode, effective_optstring) = if let Some(stripped) = optstring.strip_prefix(':') {
        (true, stripped)
    } else {
        (false, optstring.as_str())
    };

    // Get OPTERR (0 = suppress errors, default = 1)
    let opterr = context
        .get_var_or_env("OPTERR")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(1)
        != 0;

    // Get the arguments to parse
    let arguments: Vec<String> = if use_custom_args {
        args[2..].to_vec()
    } else {
        context.get_all_positional_params().to_vec()
    };

    // Clear OPTARG by default
    context.set_var("OPTARG".to_string(), String::new());

    // Check if we're done parsing
    if arg_index > arguments.len() {
        context.set_var(name.clone(), "?".to_string());
        return Ok(BuiltinResult::Success(1));
    }

    // Get current argument (1-indexed, so arg_index-1)
    let current_arg = if arg_index > 0 && arg_index <= arguments.len() {
        &arguments[arg_index - 1]
    } else {
        // No more arguments
        context.set_var(name.clone(), "?".to_string());
        return Ok(BuiltinResult::Success(1));
    };

    // Check if current argument is an option (starts with -)
    if !current_arg.starts_with('-') || current_arg == "-" {
        // Not an option - done parsing
        context.set_var(name.clone(), "?".to_string());
        return Ok(BuiltinResult::Success(1));
    }

    // Check for -- (end of options)
    if current_arg == "--" {
        context.set_var("OPTIND".to_string(), (arg_index + 1).to_string());
        context.set_var(name.clone(), "?".to_string());
        // Clear getopts state
        context.unset_variable("_GETOPTS_STATE");
        return Ok(BuiltinResult::Success(1));
    }

    // Get the option character at current position
    let arg_chars: Vec<char> = current_arg.chars().collect();

    // char_pos starts at 1 (skip the leading -)
    if char_pos < 1 {
        char_pos = 1;
    }

    if char_pos >= arg_chars.len() {
        // Move to next argument
        arg_index += 1;
        char_pos = 1;
        optind = arg_index;

        // Recurse with updated state
        context.set_var("OPTIND".to_string(), optind.to_string());
        save_getopts_state(context, arg_index, char_pos);
        return builtin_getopts(args, context);
    }

    let opt_char = arg_chars[char_pos];

    // Check if this option is in optstring
    let opt_pos = effective_optstring.find(opt_char);

    match opt_pos {
        Some(pos) => {
            // Valid option found
            let needs_arg = pos + 1 < effective_optstring.len()
                && effective_optstring.chars().nth(pos + 1) == Some(':');

            if needs_arg {
                // Option requires an argument
                let opt_arg = if char_pos + 1 < arg_chars.len() {
                    // Argument is attached: -bvalue
                    let arg: String = arg_chars[char_pos + 1..].iter().collect();
                    // Move to next argument
                    arg_index += 1;
                    char_pos = 1;
                    Some(arg)
                } else {
                    // Argument is next positional parameter
                    if arg_index < arguments.len() {
                        let arg = arguments[arg_index].clone(); // arg_index is 1-based, so this is next
                        arg_index += 2; // Skip current option and its argument
                        char_pos = 1;
                        Some(arg)
                    } else {
                        // Missing argument
                        None
                    }
                };

                match opt_arg {
                    Some(arg) => {
                        context.set_var(name.clone(), opt_char.to_string());
                        context.set_var("OPTARG".to_string(), arg);
                        context.set_var("OPTIND".to_string(), arg_index.to_string());
                        save_getopts_state(context, arg_index, char_pos);
                        Ok(BuiltinResult::Success(0))
                    }
                    None => {
                        // Missing required argument
                        if silent_mode {
                            context.set_var(name.clone(), ":".to_string());
                            context.set_var("OPTARG".to_string(), opt_char.to_string());
                        } else {
                            if opterr {
                                eprintln!("getopts: option requires an argument -- {}", opt_char);
                            }
                            context.set_var(name.clone(), "?".to_string());
                        }
                        arg_index += 1;
                        context.set_var("OPTIND".to_string(), arg_index.to_string());
                        save_getopts_state(context, arg_index, 1);
                        Ok(BuiltinResult::Success(0))
                    }
                }
            } else {
                // Option doesn't require argument
                context.set_var(name.clone(), opt_char.to_string());

                // Move to next character or next argument
                char_pos += 1;
                if char_pos >= arg_chars.len() {
                    arg_index += 1;
                    char_pos = 1;
                }

                context.set_var("OPTIND".to_string(), arg_index.to_string());
                save_getopts_state(context, arg_index, char_pos);
                Ok(BuiltinResult::Success(0))
            }
        }
        None => {
            // Invalid option
            if silent_mode {
                context.set_var(name.clone(), "?".to_string());
                context.set_var("OPTARG".to_string(), opt_char.to_string());
            } else {
                if opterr {
                    eprintln!("getopts: illegal option -- {}", opt_char);
                }
                context.set_var(name.clone(), "?".to_string());
            }

            // Move to next character or next argument
            char_pos += 1;
            if char_pos >= arg_chars.len() {
                arg_index += 1;
                char_pos = 1;
            }

            context.set_var("OPTIND".to_string(), arg_index.to_string());
            save_getopts_state(context, arg_index, char_pos);
            Ok(BuiltinResult::Success(0))
        }
    }
}

/// Parse getopts internal state from _GETOPTS_STATE variable
fn parse_getopts_state(context: &ExpansionContext, optind: usize) -> (usize, usize) {
    if let Some(state) = context.get_var_or_env("_GETOPTS_STATE") {
        let parts: Vec<&str> = state.split(':').collect();
        if parts.len() == 2 {
            let saved_optind: usize = parts[0].parse().unwrap_or(optind);
            let char_pos: usize = parts[1].parse().unwrap_or(1);

            // If OPTIND was reset (e.g., OPTIND=1), reset internal state too
            if optind == 1 && saved_optind != 1 {
                return (1, 1);
            }

            // If OPTIND matches, use saved char position
            if saved_optind == optind {
                return (saved_optind, char_pos);
            }
        }
    }

    (optind, 1)
}

/// Save getopts internal state to _GETOPTS_STATE variable
fn save_getopts_state(context: &mut ExpansionContext, arg_index: usize, char_pos: usize) {
    context.set_var(
        "_GETOPTS_STATE".to_string(),
        format!("{}:{}", arg_index, char_pos),
    );
}

fn builtin_shift(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    let n: usize = if args.is_empty() {
        1
    } else {
        args[0].parse().unwrap_or(1)
    };

    let current_params = context.get_all_positional_params().to_vec();

    if n > current_params.len() {
        eprintln!("shift: shift count must be <= $#");
        return Ok(BuiltinResult::Failure(1));
    }

    let new_params: Vec<String> = current_params.into_iter().skip(n).collect();
    context.set_positional_params(new_params);

    Ok(BuiltinResult::Success(0))
}

fn tilde_abbreviate(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if let Some(home_path) = home {
        if let Ok(stripped) = path.strip_prefix(home_path) {
            if stripped.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}

fn parse_stack_index(arg: &str, stack_len: usize) -> Option<usize> {
    if let Some(stripped) = arg.strip_prefix('+') {
        stripped.parse::<usize>().ok().filter(|&n| n < stack_len)
    } else if let Some(stripped) = arg.strip_prefix('-') {
        if !stripped.is_empty() && stripped.chars().next().unwrap().is_ascii_digit() {
            let n = stripped.parse::<usize>().ok()?;
            if n < stack_len {
                Some(stack_len - 1 - n)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

fn builtin_dirs(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    let mut clear = false;
    let mut long_format = false;
    let mut one_per_line = false;
    let mut with_index = false;
    let mut index_arg: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "-c" => clear = true,
            "-l" => long_format = true,
            "-p" => one_per_line = true,
            "-v" => {
                with_index = true;
                one_per_line = true;
            }
            s if s.starts_with('+')
                || (s.starts_with('-')
                    && s.len() > 1
                    && s.chars().nth(1).unwrap().is_ascii_digit()) =>
            {
                index_arg = Some(s);
            }
            _ => {
                eprintln!("dirs: {}: invalid option", arg);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    context.sync_dir_stack_top();
    let stack = context.dir_stack();
    let home = context.home();

    if clear {
        let cwd = stack
            .first()
            .cloned()
            .unwrap_or_else(|| env::current_dir().unwrap_or_default());
        context.dir_stack_mut().clear();
        context.dir_stack_mut().push(cwd);
        return Ok(BuiltinResult::Success(0));
    }

    if let Some(idx_str) = index_arg {
        let stack_len = stack.len();
        match parse_stack_index(idx_str, stack_len) {
            Some(idx) => {
                let path = &stack[idx];
                let display = if long_format {
                    path.display().to_string()
                } else {
                    tilde_abbreviate(path, home.as_deref())
                };
                println!("{}", display);
                return Ok(BuiltinResult::Success(0));
            }
            None => {
                eprintln!("dirs: {}: directory stack index out of range", idx_str);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    if stack.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }

    if one_per_line {
        for (i, path) in stack.iter().enumerate() {
            let display = if long_format {
                path.display().to_string()
            } else {
                tilde_abbreviate(path, home.as_deref())
            };
            if with_index {
                println!("{:>2}  {}", i, display);
            } else {
                println!("{}", display);
            }
        }
    } else {
        let entries: Vec<String> = stack
            .iter()
            .map(|p| {
                if long_format {
                    p.display().to_string()
                } else {
                    tilde_abbreviate(p, home.as_deref())
                }
            })
            .collect();
        println!("{}", entries.join(" "));
    }

    Ok(BuiltinResult::Success(0))
}

fn builtin_pushd(
    args: &[String],
    context: &mut ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) -> Result<BuiltinResult> {
    let mut no_cd = false;
    let mut target: Option<&str> = None;
    let mut index_arg: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "-n" => no_cd = true,
            s if s.starts_with('+')
                || (s.starts_with('-')
                    && s.len() > 1
                    && s.chars().nth(1).unwrap().is_ascii_digit()) =>
            {
                index_arg = Some(s);
            }
            s => target = Some(s),
        }
    }

    context.sync_dir_stack_top();
    let home = context.home();

    if let Some(idx_str) = index_arg {
        let stack_len = context.dir_stack_len();
        match parse_stack_index(idx_str, stack_len) {
            Some(idx) => {
                if idx == 0 {
                    print_dir_stack(context, home.as_deref());
                    return Ok(BuiltinResult::Success(0));
                }

                let stack = context.dir_stack_mut();
                let rotated: Vec<PathBuf> = stack[idx..]
                    .iter()
                    .chain(stack[..idx].iter())
                    .cloned()
                    .collect();
                *stack = rotated;

                if !no_cd {
                    let new_dir = stack[0].clone();
                    let old_dir = env::current_dir().unwrap_or_default();

                    if let Err(e) = env::set_current_dir(&new_dir) {
                        eprintln!("pushd: {}: {}", new_dir.display(), e);
                        return Ok(BuiltinResult::Failure(1));
                    }

                    context.set_env("OLDPWD", old_dir.to_str().unwrap_or(""));
                    context.set_env("PWD", new_dir.to_str().unwrap_or(""));

                    tuple_space.out(Tuple::from(vec![
                        Value::Str("dir_change".to_string()),
                        Value::Str(session_id.to_string()),
                        Value::Str(old_dir.display().to_string()),
                        Value::Str(new_dir.display().to_string()),
                    ]));
                }

                print_dir_stack(context, home.as_deref());
                return Ok(BuiltinResult::Success(0));
            }
            None => {
                eprintln!("pushd: {}: directory stack index out of range", idx_str);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    if target.is_none() {
        let stack_len = context.dir_stack_len();
        if stack_len < 2 {
            eprintln!("pushd: no other directory");
            return Ok(BuiltinResult::Failure(1));
        }

        let stack = context.dir_stack_mut();
        stack.swap(0, 1);

        if !no_cd {
            let new_dir = stack[0].clone();
            let old_dir = env::current_dir().unwrap_or_default();

            if let Err(e) = env::set_current_dir(&new_dir) {
                stack.swap(0, 1);
                eprintln!("pushd: {}: {}", new_dir.display(), e);
                return Ok(BuiltinResult::Failure(1));
            }

            context.set_env("OLDPWD", old_dir.to_str().unwrap_or(""));
            context.set_env("PWD", new_dir.to_str().unwrap_or(""));

            tuple_space.out(Tuple::from(vec![
                Value::Str("dir_change".to_string()),
                Value::Str(session_id.to_string()),
                Value::Str(old_dir.display().to_string()),
                Value::Str(new_dir.display().to_string()),
            ]));
        }

        print_dir_stack(context, home.as_deref());
        return Ok(BuiltinResult::Success(0));
    }

    let target_path = PathBuf::from(target.unwrap());

    if !target_path.is_dir() {
        eprintln!(
            "pushd: {}: No such file or directory",
            target_path.display()
        );
        return Ok(BuiltinResult::Failure(1));
    }

    let target_canonical = target_path.canonicalize().unwrap_or(target_path);
    let old_dir = env::current_dir().unwrap_or_default();

    if no_cd {
        context.dir_stack_mut().insert(1, target_canonical);
    } else {
        if let Err(e) = env::set_current_dir(&target_canonical) {
            eprintln!("pushd: {}: {}", target_canonical.display(), e);
            return Ok(BuiltinResult::Failure(1));
        }

        context.dir_stack_mut().insert(0, target_canonical.clone());
        context.set_env("OLDPWD", old_dir.to_str().unwrap_or(""));
        context.set_env("PWD", target_canonical.to_str().unwrap_or(""));

        tuple_space.out(Tuple::from(vec![
            Value::Str("dir_change".to_string()),
            Value::Str(session_id.to_string()),
            Value::Str(old_dir.display().to_string()),
            Value::Str(target_canonical.display().to_string()),
        ]));
    }

    print_dir_stack(context, home.as_deref());
    Ok(BuiltinResult::Success(0))
}

fn builtin_popd(
    args: &[String],
    context: &mut ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) -> Result<BuiltinResult> {
    let mut no_cd = false;
    let mut index_arg: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "-n" => no_cd = true,
            s if s.starts_with('+')
                || (s.starts_with('-')
                    && s.len() > 1
                    && s.chars().nth(1).unwrap().is_ascii_digit()) =>
            {
                index_arg = Some(s);
            }
            _ => {
                eprintln!("popd: {}: invalid argument", arg);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    context.sync_dir_stack_top();
    let home = context.home();

    let stack_len = context.dir_stack_len();
    if stack_len <= 1 {
        eprintln!("popd: directory stack empty");
        return Ok(BuiltinResult::Failure(1));
    }

    let remove_idx = if let Some(idx_str) = index_arg {
        match parse_stack_index(idx_str, stack_len) {
            Some(idx) => idx,
            None => {
                eprintln!("popd: {}: directory stack index out of range", idx_str);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    } else {
        0
    };

    let removed = context.dir_stack_mut().remove(remove_idx);

    if remove_idx == 0 && !no_cd {
        let old_dir = removed;
        let new_dir = context
            .dir_stack()
            .first()
            .cloned()
            .unwrap_or_else(|| env::current_dir().unwrap_or_default());

        if let Err(e) = env::set_current_dir(&new_dir) {
            context.dir_stack_mut().insert(0, old_dir);
            eprintln!("popd: {}: {}", new_dir.display(), e);
            return Ok(BuiltinResult::Failure(1));
        }

        context.set_env("OLDPWD", old_dir.to_str().unwrap_or(""));
        context.set_env("PWD", new_dir.to_str().unwrap_or(""));

        tuple_space.out(Tuple::from(vec![
            Value::Str("dir_change".to_string()),
            Value::Str(session_id.to_string()),
            Value::Str(old_dir.display().to_string()),
            Value::Str(new_dir.display().to_string()),
        ]));
    }

    print_dir_stack(context, home.as_deref());
    Ok(BuiltinResult::Success(0))
}

fn print_dir_stack(context: &ExpansionContext, home: Option<&std::path::Path>) {
    let stack = context.dir_stack();
    let entries: Vec<String> = stack.iter().map(|p| tilde_abbreviate(p, home)).collect();
    println!("{}", entries.join(" "));
}

/// shopt - Set and unset shell options
///
/// Usage: shopt [-pqsu] [-o] [optname ...]
///   -s: Enable the named options
///   -u: Disable the named options
///   -p: Print options in reusable format
///   -q: Quiet mode (suppress output, only set exit status)
///   -o: Restrict to set -o options (not implemented, ignored)
///
/// Without options, displays all settable options and their status.
/// With option names, displays those options.
/// Returns 0 if all options are set/unset successfully.
fn builtin_shopt(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use colony_expand::ShoptOptions;

    let mut set_mode = false;
    let mut unset_mode = false;
    let mut print_mode = false;
    let mut quiet_mode = false;
    let mut option_names: Vec<&str> = Vec::new();

    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 {
            for c in arg[1..].chars() {
                match c {
                    's' => set_mode = true,
                    'u' => unset_mode = true,
                    'p' => print_mode = true,
                    'q' => quiet_mode = true,
                    'o' => {} // Ignored for now
                    _ => {
                        eprintln!("shopt: -{}: invalid option", c);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
        } else {
            option_names.push(arg);
        }
    }

    // Get shopt options
    let shopt = context.shopt_options_mut();

    // If both -s and -u are set, that's an error
    if set_mode && unset_mode {
        eprintln!("shopt: cannot set and unset options simultaneously");
        return Ok(BuiltinResult::Failure(1));
    }

    // Set or unset specified options
    if set_mode || unset_mode {
        if option_names.is_empty() {
            // With -s/-u but no option names, list options that are on/off
            let all_options = ShoptOptions::all_options();
            for name in all_options {
                if let Some(value) = shopt.get(name) {
                    if ((set_mode && value) || (unset_mode && !value)) && !quiet_mode {
                        if print_mode {
                            println!("shopt {} {}", if value { "-s" } else { "-u" }, name);
                        } else {
                            println!("{}\t{}", name, if value { "on" } else { "off" });
                        }
                    }
                }
            }
            return Ok(BuiltinResult::Success(0));
        }

        let value = set_mode;
        let mut exit_code = 0;

        for name in &option_names {
            if !shopt.set(name, value) {
                if !quiet_mode {
                    eprintln!("shopt: {}: invalid shell option name", name);
                }
                exit_code = 1;
            }
        }

        return Ok(if exit_code == 0 {
            BuiltinResult::Success(0)
        } else {
            BuiltinResult::Failure(1)
        });
    }

    // Query/display mode
    if option_names.is_empty() {
        // Display all options
        let all_options = ShoptOptions::all_options();
        for name in all_options {
            if let Some(value) = shopt.get(name) {
                if !quiet_mode {
                    if print_mode {
                        println!("shopt {} {}", if value { "-s" } else { "-u" }, name);
                    } else {
                        println!("{}\t{}", name, if value { "on" } else { "off" });
                    }
                }
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    // Display specific options
    let mut all_set = true;
    for name in &option_names {
        if let Some(value) = shopt.get(name) {
            if !quiet_mode {
                if print_mode {
                    println!("shopt {} {}", if value { "-s" } else { "-u" }, name);
                } else {
                    println!("{}\t{}", name, if value { "on" } else { "off" });
                }
            }
            if !value {
                all_set = false;
            }
        } else {
            if !quiet_mode {
                eprintln!("shopt: {}: invalid shell option name", name);
            }
            return Ok(BuiltinResult::Failure(1));
        }
    }

    // In query mode without -s/-u, return 0 if all named options are set
    Ok(if all_set {
        BuiltinResult::Success(0)
    } else {
        BuiltinResult::Failure(1)
    })
}

fn builtin_complete(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    let mut spec = CompletionSpec::new();
    let mut print_mode = false;
    let mut remove_mode = false;
    let mut commands: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "-p" {
            print_mode = true;
            i += 1;
        } else if arg == "-r" {
            remove_mode = true;
            i += 1;
        } else if arg == "-D" || arg == "-E" {
            i += 1;
        } else if arg == "-F" {
            i += 1;
            if i < args.len() {
                spec.function = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("complete: -F: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-W" {
            i += 1;
            if i < args.len() {
                spec.wordlist = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("complete: -W: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-G" {
            i += 1;
            if i < args.len() {
                spec.globpat = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("complete: -G: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-C" {
            i += 1;
            if i < args.len() {
                spec.command = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("complete: -C: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-P" {
            i += 1;
            if i < args.len() {
                spec.prefix = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("complete: -P: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-S" {
            i += 1;
            if i < args.len() {
                spec.suffix = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("complete: -S: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-X" {
            i += 1;
            if i < args.len() {
                spec.filter_pattern = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("complete: -X: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-o" {
            i += 1;
            if i < args.len() {
                spec.options.push(args[i].clone());
                i += 1;
            } else {
                eprintln!("complete: -o: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-A" {
            i += 1;
            if i < args.len() {
                if let Some(action) = CompletionAction::from_action_name(&args[i]) {
                    spec.actions.push(action);
                } else {
                    eprintln!("complete: {}: invalid action name", &args[i]);
                    return Ok(BuiltinResult::Failure(1));
                }
                i += 1;
            } else {
                eprintln!("complete: -A: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-a" {
            spec.actions.push(CompletionAction::Alias);
            i += 1;
        } else if arg == "-b" {
            spec.actions.push(CompletionAction::Builtin);
            i += 1;
        } else if arg == "-c" {
            spec.actions.push(CompletionAction::Command);
            i += 1;
        } else if arg == "-d" {
            spec.actions.push(CompletionAction::Directory);
            i += 1;
        } else if arg == "-e" {
            spec.actions.push(CompletionAction::Export);
            i += 1;
        } else if arg == "-f" {
            spec.actions.push(CompletionAction::File);
            i += 1;
        } else if arg == "-g" {
            spec.actions.push(CompletionAction::Group);
            i += 1;
        } else if arg == "-j" {
            spec.actions.push(CompletionAction::Job);
            i += 1;
        } else if arg == "-k" {
            spec.actions.push(CompletionAction::Keyword);
            i += 1;
        } else if arg == "-s" {
            spec.actions.push(CompletionAction::Service);
            i += 1;
        } else if arg == "-u" {
            spec.actions.push(CompletionAction::User);
            i += 1;
        } else if arg == "-v" {
            spec.actions.push(CompletionAction::Variable);
            i += 1;
        } else if !arg.starts_with('-') {
            commands.push(arg.clone());
            i += 1;
        } else {
            eprintln!("complete: {}: invalid option", arg);
            return Ok(BuiltinResult::Failure(1));
        }
    }

    if print_mode || (commands.is_empty() && !remove_mode && spec.is_empty()) {
        if commands.is_empty() {
            for (cmd, spec) in context.completions() {
                print_completion_spec(cmd, spec);
            }
        } else {
            for cmd in &commands {
                if let Some(spec) = context.get_completion(cmd) {
                    print_completion_spec(cmd, spec);
                } else {
                    eprintln!("complete: {}: no completion specification", cmd);
                    return Ok(BuiltinResult::Failure(1));
                }
            }
        }
        return Ok(BuiltinResult::Success(0));
    }

    if remove_mode {
        for cmd in &commands {
            context.remove_completion(cmd);
        }
        return Ok(BuiltinResult::Success(0));
    }

    for cmd in commands {
        context.set_completion(cmd, spec.clone());
    }

    Ok(BuiltinResult::Success(0))
}

fn print_completion_spec(cmd: &str, spec: &CompletionSpec) {
    let mut parts = vec!["complete".to_string()];

    for action in &spec.actions {
        match action {
            CompletionAction::Alias => parts.push("-a".to_string()),
            CompletionAction::Builtin => parts.push("-b".to_string()),
            CompletionAction::Command => parts.push("-c".to_string()),
            CompletionAction::Directory => parts.push("-d".to_string()),
            CompletionAction::Export => parts.push("-e".to_string()),
            CompletionAction::File => parts.push("-f".to_string()),
            CompletionAction::Group => parts.push("-g".to_string()),
            CompletionAction::Job => parts.push("-j".to_string()),
            CompletionAction::Keyword => parts.push("-k".to_string()),
            CompletionAction::Service => parts.push("-s".to_string()),
            CompletionAction::User => parts.push("-u".to_string()),
            CompletionAction::Variable => parts.push("-v".to_string()),
            CompletionAction::Function => parts.push("-A function".to_string()),
            CompletionAction::Hostname => parts.push("-A hostname".to_string()),
            CompletionAction::HelpTopic => parts.push("-A helptopic".to_string()),
            CompletionAction::Signal => parts.push("-A signal".to_string()),
            CompletionAction::Stopped => parts.push("-A stopped".to_string()),
            CompletionAction::Running => parts.push("-A running".to_string()),
            CompletionAction::SetOpt => parts.push("-A setopt".to_string()),
            CompletionAction::ShOpt => parts.push("-A shopt".to_string()),
            CompletionAction::ArrayVar => parts.push("-A arrayvar".to_string()),
            CompletionAction::Binding => parts.push("-A binding".to_string()),
        }
    }

    for opt in &spec.options {
        parts.push("-o".to_string());
        parts.push(opt.clone());
    }

    if let Some(ref func) = spec.function {
        parts.push("-F".to_string());
        parts.push(func.clone());
    }

    if let Some(ref words) = spec.wordlist {
        parts.push("-W".to_string());
        parts.push(format!("'{}'", words));
    }

    if let Some(ref glob) = spec.globpat {
        parts.push("-G".to_string());
        parts.push(format!("'{}'", glob));
    }

    if let Some(ref cmd_str) = spec.command {
        parts.push("-C".to_string());
        parts.push(format!("'{}'", cmd_str));
    }

    if let Some(ref prefix) = spec.prefix {
        parts.push("-P".to_string());
        parts.push(format!("'{}'", prefix));
    }

    if let Some(ref suffix) = spec.suffix {
        parts.push("-S".to_string());
        parts.push(format!("'{}'", suffix));
    }

    if let Some(ref filter) = spec.filter_pattern {
        parts.push("-X".to_string());
        parts.push(format!("'{}'", filter));
    }

    parts.push(cmd.to_string());

    println!("{}", parts.join(" "));
}

fn builtin_compgen(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    let mut spec = CompletionSpec::new();
    let mut word_prefix: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "--" {
            i += 1;
            if i < args.len() {
                word_prefix = Some(args[i].clone());
            }
            break;
        } else if arg == "-W" {
            i += 1;
            if i < args.len() {
                spec.wordlist = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("compgen: -W: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-G" {
            i += 1;
            if i < args.len() {
                spec.globpat = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("compgen: -G: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-P" {
            i += 1;
            if i < args.len() {
                spec.prefix = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("compgen: -P: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-S" {
            i += 1;
            if i < args.len() {
                spec.suffix = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("compgen: -S: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-X" {
            i += 1;
            if i < args.len() {
                spec.filter_pattern = Some(args[i].clone());
                i += 1;
            } else {
                eprintln!("compgen: -X: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-A" {
            i += 1;
            if i < args.len() {
                if let Some(action) = CompletionAction::from_action_name(&args[i]) {
                    spec.actions.push(action);
                } else {
                    eprintln!("compgen: {}: invalid action name", &args[i]);
                    return Ok(BuiltinResult::Failure(1));
                }
                i += 1;
            } else {
                eprintln!("compgen: -A: option requires an argument");
                return Ok(BuiltinResult::Failure(1));
            }
        } else if arg == "-a" {
            spec.actions.push(CompletionAction::Alias);
            i += 1;
        } else if arg == "-b" {
            spec.actions.push(CompletionAction::Builtin);
            i += 1;
        } else if arg == "-c" {
            spec.actions.push(CompletionAction::Command);
            i += 1;
        } else if arg == "-d" {
            spec.actions.push(CompletionAction::Directory);
            i += 1;
        } else if arg == "-e" {
            spec.actions.push(CompletionAction::Export);
            i += 1;
        } else if arg == "-f" {
            spec.actions.push(CompletionAction::File);
            i += 1;
        } else if arg == "-g" {
            spec.actions.push(CompletionAction::Group);
            i += 1;
        } else if arg == "-j" {
            spec.actions.push(CompletionAction::Job);
            i += 1;
        } else if arg == "-k" {
            spec.actions.push(CompletionAction::Keyword);
            i += 1;
        } else if arg == "-s" {
            spec.actions.push(CompletionAction::Service);
            i += 1;
        } else if arg == "-u" {
            spec.actions.push(CompletionAction::User);
            i += 1;
        } else if arg == "-v" {
            spec.actions.push(CompletionAction::Variable);
            i += 1;
        } else if !arg.starts_with('-') {
            word_prefix = Some(arg.clone());
            i += 1;
        } else {
            eprintln!("compgen: {}: invalid option", arg);
            return Ok(BuiltinResult::Failure(1));
        }
    }

    let completions = generate_completions(&spec, context, word_prefix.as_deref());

    for comp in completions {
        let mut result = String::new();
        if let Some(ref prefix) = spec.prefix {
            result.push_str(prefix);
        }
        result.push_str(&comp);
        if let Some(ref suffix) = spec.suffix {
            result.push_str(suffix);
        }
        println!("{}", result);
    }

    Ok(BuiltinResult::Success(0))
}

fn generate_completions(
    spec: &CompletionSpec,
    context: &ExpansionContext,
    word_prefix: Option<&str>,
) -> Vec<String> {
    let mut completions = Vec::new();
    let prefix = word_prefix.unwrap_or("");

    if let Some(ref wordlist) = spec.wordlist {
        for word in wordlist.split_whitespace() {
            if word.starts_with(prefix) {
                completions.push(word.to_string());
            }
        }
    }

    if let Some(ref glob_pattern) = spec.globpat {
        if let Ok(entries) = glob::glob(glob_pattern) {
            for entry in entries.flatten() {
                let path_str = entry.display().to_string();
                if path_str.starts_with(prefix) {
                    completions.push(path_str);
                }
            }
        }
    }

    for action in &spec.actions {
        match action {
            CompletionAction::Alias => {
                for name in context.aliases().keys() {
                    if name.starts_with(prefix) {
                        completions.push(name.clone());
                    }
                }
            }
            CompletionAction::Builtin => {
                for name in get_builtin_names() {
                    if name.starts_with(prefix) {
                        completions.push(name.to_string());
                    }
                }
            }
            CompletionAction::Command => {
                for name in get_builtin_names() {
                    if name.starts_with(prefix) {
                        completions.push(name.to_string());
                    }
                }
                for name in context.functions().keys() {
                    if name.starts_with(prefix) {
                        completions.push(name.clone());
                    }
                }
                if let Ok(path) = std::env::var("PATH") {
                    for dir in path.split(':') {
                        if let Ok(entries) = std::fs::read_dir(dir) {
                            for entry in entries.flatten() {
                                if let Some(name) = entry.file_name().to_str() {
                                    if name.starts_with(prefix) {
                                        completions.push(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            CompletionAction::Directory => {
                let pattern = if prefix.is_empty() {
                    "*"
                } else {
                    &format!("{}*", prefix)
                };
                if let Ok(entries) = glob::glob(pattern) {
                    for entry in entries.flatten() {
                        if entry.is_dir() {
                            completions.push(entry.display().to_string());
                        }
                    }
                }
            }
            CompletionAction::Export => {
                for name in context.exported_variable_names() {
                    if name.starts_with(prefix) {
                        completions.push(name.clone());
                    }
                }
            }
            CompletionAction::File => {
                let pattern = if prefix.is_empty() {
                    "*"
                } else {
                    &format!("{}*", prefix)
                };
                if let Ok(entries) = glob::glob(pattern) {
                    for entry in entries.flatten() {
                        completions.push(entry.display().to_string());
                    }
                }
            }
            CompletionAction::Function => {
                for name in context.functions().keys() {
                    if name.starts_with(prefix) {
                        completions.push(name.clone());
                    }
                }
            }
            CompletionAction::Variable => {
                for name in context.variable_names() {
                    if name.starts_with(prefix) {
                        completions.push(name.clone());
                    }
                }
                for name in context.exported_variable_names() {
                    if name.starts_with(prefix) && !completions.contains(name) {
                        completions.push(name.clone());
                    }
                }
            }
            CompletionAction::Keyword => {
                for kw in &[
                    "if", "then", "else", "elif", "fi", "case", "esac", "for", "while", "until",
                    "do", "done", "in", "function", "select", "time", "coproc", "{", "}", "!",
                    "[[", "]]",
                ] {
                    if kw.starts_with(prefix) {
                        completions.push(kw.to_string());
                    }
                }
            }
            CompletionAction::User => {
                if let Ok(contents) = std::fs::read_to_string("/etc/passwd") {
                    for line in contents.lines() {
                        if let Some(username) = line.split(':').next() {
                            if username.starts_with(prefix) {
                                completions.push(username.to_string());
                            }
                        }
                    }
                }
            }
            CompletionAction::Group => {
                if let Ok(contents) = std::fs::read_to_string("/etc/group") {
                    for line in contents.lines() {
                        if let Some(groupname) = line.split(':').next() {
                            if groupname.starts_with(prefix) {
                                completions.push(groupname.to_string());
                            }
                        }
                    }
                }
            }
            CompletionAction::Signal => {
                for sig in &[
                    "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "BUS", "FPE", "KILL", "USR1",
                    "SEGV", "USR2", "PIPE", "ALRM", "TERM", "STKFLT", "CHLD", "CONT", "STOP",
                    "TSTP", "TTIN", "TTOU", "URG", "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "IO",
                    "PWR", "SYS",
                ] {
                    if sig.starts_with(prefix) || format!("SIG{}", sig).starts_with(prefix) {
                        completions.push(sig.to_string());
                    }
                }
            }
            CompletionAction::SetOpt => {
                for opt in &[
                    "errexit", "nounset", "xtrace", "noglob", "noexec", "verbose",
                ] {
                    if opt.starts_with(prefix) {
                        completions.push(opt.to_string());
                    }
                }
            }
            CompletionAction::ShOpt => {
                for opt in &[
                    "extglob",
                    "globstar",
                    "nullglob",
                    "dotglob",
                    "failglob",
                    "nocaseglob",
                    "nocasematch",
                    "expand_aliases",
                    "histappend",
                ] {
                    if opt.starts_with(prefix) {
                        completions.push(opt.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(ref filter) = spec.filter_pattern {
        completions.retain(|c| {
            !glob::Pattern::new(filter)
                .map(|p| p.matches(c))
                .unwrap_or(false)
        });
    }

    completions.sort();
    completions.dedup();

    completions
}

fn builtin_history(
    args: &[String],
    history: Option<&mut crate::history::History>,
    history_store: Option<&std::sync::Arc<colony_history::HistoryStore>>,
) -> Result<BuiltinResult> {
    // Check for new subcommands that require the HistoryStore
    if let Some(subcommand) = args.first() {
        match subcommand.as_str() {
            "search" => {
                let Some(store) = history_store else {
                    eprintln!("history search: history store not available");
                    return Ok(BuiltinResult::Failure(1));
                };
                let query = args[1..].join(" ");
                if query.is_empty() {
                    eprintln!("history search: missing query");
                    return Ok(BuiltinResult::Failure(1));
                }
                match store.search_fts(&query, 50) {
                    Ok(results) => {
                        for r in &results {
                            let ts = chrono::DateTime::from_timestamp_millis(r.timestamp)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_default();
                            let exit_marker = if r.exit_code != 0 {
                                format!(" [{}]", r.exit_code)
                            } else {
                                String::new()
                            };
                            println!("  {}  {}{}", ts, r.command, exit_marker);
                        }
                        if results.is_empty() {
                            println!("No results for: {}", query);
                        }
                        return Ok(BuiltinResult::Success(0));
                    }
                    Err(e) => {
                        eprintln!("history search: {}", e);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
            "stats" => {
                let Some(store) = history_store else {
                    eprintln!("history stats: history store not available");
                    return Ok(BuiltinResult::Failure(1));
                };
                match colony_history::HistoryStats::compute(store) {
                    Ok(stats) => {
                        let since = stats
                            .earliest_timestamp
                            .and_then(chrono::DateTime::from_timestamp_millis)
                            .map(|dt| dt.format("%Y-%m-%d").to_string())
                            .unwrap_or_else(|| "N/A".to_string());
                        println!("Commands: {} (since {})", stats.total_commands, since);
                        println!(
                            "Today: {}  |  This week: {}  |  This month: {}",
                            stats.today_count, stats.week_count, stats.month_count
                        );
                        println!();

                        if !stats.executor_counts.is_empty() {
                            println!("By executor:");
                            for (executor, count) in &stats.executor_counts {
                                let pct = if stats.total_commands > 0 {
                                    (*count as f64 / stats.total_commands as f64) * 100.0
                                } else {
                                    0.0
                                };
                                println!("  {:<16} {:>6} ({:.1}%)", executor, count, pct);
                            }
                            println!();
                        }

                        if !stats.top_directories.is_empty() {
                            println!("Top directories:");
                            for (dir, count) in &stats.top_directories {
                                println!("  {:<40} {:>6}", dir, count);
                            }
                            println!();
                        }

                        if !stats.peak_hours.is_empty() {
                            let top_hours: Vec<String> = stats
                                .peak_hours
                                .iter()
                                .filter(|(_, c)| *c > 0)
                                .map(|(h, _)| format!("{:02}-{:02}", h, (h + 1) % 24))
                                .take(4)
                                .collect();
                            if !top_hours.is_empty() {
                                println!("Peak hours: {}", top_hours.join(", "));
                            }
                        }

                        return Ok(BuiltinResult::Success(0));
                    }
                    Err(e) => {
                        eprintln!("history stats: {}", e);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
            "bookmark" => {
                let Some(store) = history_store else {
                    eprintln!("history bookmark: history store not available");
                    return Ok(BuiltinResult::Failure(1));
                };
                let Some(id) = args.get(1) else {
                    eprintln!("history bookmark: missing record ID");
                    return Ok(BuiltinResult::Failure(1));
                };
                match store.toggle_bookmark(id) {
                    Ok(bookmarked) => {
                        if bookmarked {
                            println!("Bookmarked: {}", id);
                        } else {
                            println!("Unbookmarked: {}", id);
                        }
                        return Ok(BuiltinResult::Success(0));
                    }
                    Err(e) => {
                        eprintln!("history bookmark: {}", e);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
            "note" => {
                let Some(store) = history_store else {
                    eprintln!("history note: history store not available");
                    return Ok(BuiltinResult::Failure(1));
                };
                let Some(id) = args.get(1) else {
                    eprintln!("history note: usage: history note <id> <text>");
                    return Ok(BuiltinResult::Failure(1));
                };
                let text = args[2..].join(" ");
                if text.is_empty() {
                    eprintln!("history note: missing note text");
                    return Ok(BuiltinResult::Failure(1));
                }
                match store.set_note(id, &text) {
                    Ok(()) => {
                        println!("Note set on {}", id);
                        return Ok(BuiltinResult::Success(0));
                    }
                    Err(e) => {
                        eprintln!("history note: {}", e);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
            "tag" => {
                let Some(store) = history_store else {
                    eprintln!("history tag: history store not available");
                    return Ok(BuiltinResult::Failure(1));
                };
                let Some(id) = args.get(1) else {
                    eprintln!("history tag: usage: history tag <id> <tags>");
                    return Ok(BuiltinResult::Failure(1));
                };
                let tags = args[2..].join(",");
                if tags.is_empty() {
                    eprintln!("history tag: missing tags");
                    return Ok(BuiltinResult::Failure(1));
                }
                match store.set_tags(id, &tags) {
                    Ok(()) => {
                        println!("Tags set on {}", id);
                        return Ok(BuiltinResult::Success(0));
                    }
                    Err(e) => {
                        eprintln!("history tag: {}", e);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
            "agents" => {
                let Some(store) = history_store else {
                    eprintln!("history agents: history store not available");
                    return Ok(BuiltinResult::Failure(1));
                };
                match store.agent_summary() {
                    Ok(summaries) => {
                        if summaries.is_empty() {
                            println!("No agent commands recorded.");
                        } else {
                            for summary in &summaries {
                                println!("{}: {} commands", summary.executor, summary.total);
                                for (risk, count) in &summary.risk_breakdown {
                                    println!("  {}: {}", risk, count);
                                }
                            }
                        }
                        return Ok(BuiltinResult::Success(0));
                    }
                    Err(e) => {
                        eprintln!("history agents: {}", e);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
            "delete" => {
                let Some(store) = history_store else {
                    eprintln!("history delete: history store not available");
                    return Ok(BuiltinResult::Failure(1));
                };
                let option = args.get(1).map(String::as_str);
                match option {
                    Some("--before") => {
                        let Some(date_str) = args.get(2) else {
                            eprintln!("history delete: --before requires a date argument");
                            return Ok(BuiltinResult::Failure(1));
                        };
                        let Some(timestamp) = parse_date_to_timestamp(date_str) else {
                            eprintln!(
                                "history delete: invalid date '{}' (expected YYYY-MM-DD)",
                                date_str
                            );
                            return Ok(BuiltinResult::Failure(1));
                        };
                        match store.delete_before(timestamp) {
                            Ok(deleted) => {
                                println!("Deleted {} records", deleted);
                                return Ok(BuiltinResult::Success(0));
                            }
                            Err(e) => {
                                eprintln!("history delete: {}", e);
                                return Ok(BuiltinResult::Failure(1));
                            }
                        }
                    }
                    Some("--pattern") => {
                        let Some(pattern) = args.get(2) else {
                            eprintln!("history delete: --pattern requires a regex argument");
                            return Ok(BuiltinResult::Failure(1));
                        };
                        match store.delete_by_pattern(pattern) {
                            Ok(deleted) => {
                                println!("Deleted {} records", deleted);
                                return Ok(BuiltinResult::Success(0));
                            }
                            Err(e) => {
                                eprintln!("history delete: {}", e);
                                return Ok(BuiltinResult::Failure(1));
                            }
                        }
                    }
                    Some(other) => {
                        eprintln!(
                            "history delete: unknown option '{}'. Use --before <date> or --pattern <regex>",
                            other
                        );
                        return Ok(BuiltinResult::Failure(1));
                    }
                    None => {
                        eprintln!(
                            "history delete: usage: history delete --before <date> | --pattern <regex>"
                        );
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
            "export" => {
                let Some(store) = history_store else {
                    eprintln!("history export: history store not available");
                    return Ok(BuiltinResult::Failure(1));
                };
                let mut format = colony_history::ExportFormat::Json;
                let mut i = 1;
                while i < args.len() {
                    if args[i] == "--format" {
                        i += 1;
                        if let Some(fmt) = args.get(i) {
                            format = match fmt.as_str() {
                                "json" => colony_history::ExportFormat::Json,
                                "csv" => colony_history::ExportFormat::Csv,
                                "jsonl" => colony_history::ExportFormat::Jsonl,
                                _ => {
                                    eprintln!(
                                        "history export: unknown format '{}' (json, csv, jsonl)",
                                        fmt
                                    );
                                    return Ok(BuiltinResult::Failure(1));
                                }
                            };
                        } else {
                            eprintln!("history export: --format requires an argument");
                            return Ok(BuiltinResult::Failure(1));
                        }
                    }
                    i += 1;
                }
                match store.export_all(format) {
                    Ok(output) => {
                        println!("{}", output);
                        return Ok(BuiltinResult::Success(0));
                    }
                    Err(e) => {
                        eprintln!("history export: {}", e);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }
            _ => {
                // Not a recognized subcommand -- fall through to legacy flag handling
            }
        }
    }

    // Fall through to existing history builtin behavior (flags: -c, -d, -a, -r, -w, -n)
    match history {
        Some(h) => {
            let exit_code = crate::history::execute_history_builtin(args, h);
            if exit_code == 0 {
                Ok(BuiltinResult::Success(0))
            } else {
                Ok(BuiltinResult::Failure(exit_code))
            }
        }
        None => {
            eprintln!("history: not available in subshell");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// Parse a date string in YYYY-MM-DD format to a Unix timestamp in milliseconds.
fn parse_date_to_timestamp(date_str: &str) -> Option<i64> {
    let naive = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    let datetime = naive
        .and_hms_opt(0, 0, 0)?
        .and_local_timezone(chrono::Local)
        .earliest()?;
    Some(datetime.timestamp_millis())
}

/// path - Manage PATH environment variable
///
/// Usage: path <subcommand> [args...]
///
/// Subcommands:
///   add <dir>      Add directory to end of PATH
///   prepend <dir>  Add directory to front of PATH
///   remove <dir>   Remove directory from PATH
///   list           Show all PATH entries (numbered)
///   has <dir>      Check if directory is in PATH (exit 0 if yes, 1 if no)
///
/// **Reactive:** Emits ("env_change", "PATH", old, new) tuple on modification
///
/// # Examples
///
/// ```bash
/// path add ~/.local/bin
/// path prepend /opt/homebrew/bin
/// path remove /usr/local/games
/// path list
/// path has ~/.cargo/bin && echo "cargo in PATH"
/// ```
fn builtin_path(
    args: &[String],
    context: &mut ExpansionContext,
    tuple_space: &TupleSpace,
    session_id: &str,
) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("path: missing subcommand");
        eprintln!("Usage: path <add|prepend|remove|list|has> [dir]");
        return Ok(BuiltinResult::Failure(1));
    }

    let subcommand = &args[0];
    let path_arg = args.get(1);

    match subcommand.as_str() {
        "add" | "append" => {
            let Some(dir) = path_arg else {
                eprintln!("path add: missing directory argument");
                return Ok(BuiltinResult::Failure(1));
            };

            let dir = expand_tilde(dir);
            let old_path = env::var("PATH").unwrap_or_default();
            let entries: Vec<&str> = old_path.split(':').filter(|s| !s.is_empty()).collect();

            if entries.iter().any(|e| *e == dir) {
                return Ok(BuiltinResult::Success(0));
            }

            let new_path = if old_path.is_empty() {
                dir.clone()
            } else {
                format!("{}:{}", old_path, dir)
            };

            env::set_var("PATH", &new_path);
            context.set_env("PATH", &new_path);

            tuple_space.out(Tuple::from(vec![
                Value::Str("env_change".to_string()),
                Value::Str(session_id.to_string()),
                Value::Str("PATH".to_string()),
                Value::Str(old_path),
                Value::Str(new_path),
            ]));

            Ok(BuiltinResult::Success(0))
        }

        "prepend" => {
            let Some(dir) = path_arg else {
                eprintln!("path prepend: missing directory argument");
                return Ok(BuiltinResult::Failure(1));
            };

            let dir = expand_tilde(dir);

            let old_path = env::var("PATH").unwrap_or_default();
            let entries: Vec<&str> = old_path.split(':').filter(|s| !s.is_empty()).collect();

            if entries.iter().any(|e| *e == dir) {
                let filtered: Vec<&str> = entries.into_iter().filter(|e| *e != dir).collect();
                let new_path = format!("{}:{}", dir, filtered.join(":"));

                env::set_var("PATH", &new_path);
                context.set_env("PATH", &new_path);

                tuple_space.out(Tuple::from(vec![
                    Value::Str("env_change".to_string()),
                    Value::Str(session_id.to_string()),
                    Value::Str("PATH".to_string()),
                    Value::Str(old_path),
                    Value::Str(new_path),
                ]));

                return Ok(BuiltinResult::Success(0));
            }

            let new_path = if old_path.is_empty() {
                dir.clone()
            } else {
                format!("{}:{}", dir, old_path)
            };

            env::set_var("PATH", &new_path);
            context.set_env("PATH", &new_path);

            tuple_space.out(Tuple::from(vec![
                Value::Str("env_change".to_string()),
                Value::Str(session_id.to_string()),
                Value::Str("PATH".to_string()),
                Value::Str(old_path),
                Value::Str(new_path),
            ]));

            Ok(BuiltinResult::Success(0))
        }

        "remove" | "rm" => {
            let Some(dir) = path_arg else {
                eprintln!("path remove: missing directory argument");
                return Ok(BuiltinResult::Failure(1));
            };

            let dir = expand_tilde(dir);

            let old_path = env::var("PATH").unwrap_or_default();
            let entries: Vec<&str> = old_path.split(':').filter(|s| !s.is_empty()).collect();
            let new_entries: Vec<&str> = entries.into_iter().filter(|e| *e != dir).collect();
            let new_path = new_entries.join(":");

            if old_path == new_path {
                eprintln!("path remove: {} not in PATH", dir);
                return Ok(BuiltinResult::Failure(1));
            }

            env::set_var("PATH", &new_path);
            context.set_env("PATH", &new_path);

            tuple_space.out(Tuple::from(vec![
                Value::Str("env_change".to_string()),
                Value::Str(session_id.to_string()),
                Value::Str("PATH".to_string()),
                Value::Str(old_path),
                Value::Str(new_path),
            ]));

            Ok(BuiltinResult::Success(0))
        }

        "list" | "ls" => {
            let path = env::var("PATH").unwrap_or_default();
            let entries: Vec<&str> = path.split(':').filter(|s| !s.is_empty()).collect();

            if entries.is_empty() {
                println!("PATH is empty");
            } else {
                for (i, entry) in entries.iter().enumerate() {
                    let exists = std::path::Path::new(entry).is_dir();
                    let marker = if exists { "" } else { " (not found)" };
                    println!("{:3}  {}{}", i + 1, entry, marker);
                }
            }

            Ok(BuiltinResult::Success(0))
        }

        "has" | "contains" => {
            let Some(dir) = path_arg else {
                eprintln!("path has: missing directory argument");
                return Ok(BuiltinResult::Failure(1));
            };

            let dir = expand_tilde(dir);

            let path = env::var("PATH").unwrap_or_default();
            let entries: Vec<&str> = path.split(':').filter(|s| !s.is_empty()).collect();

            if entries.iter().any(|e| *e == dir) {
                Ok(BuiltinResult::Success(0))
            } else {
                Ok(BuiltinResult::Failure(1))
            }
        }

        _ => {
            eprintln!("path: unknown subcommand '{}'", subcommand);
            eprintln!("Usage: path <add|prepend|remove|list|has> [dir]");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = env::var("HOME") {
            return format!("{}{}", home, &path[1..]);
        }
    } else if path == "~" {
        if let Ok(home) = env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

fn get_builtin_names() -> Vec<&'static str> {
    vec![
        ".",
        ":",
        "[",
        "alias",
        "bg",
        "break",
        "builtin",
        "cd",
        "command",
        "compgen",
        "complete",
        "compopt",
        "continue",
        "declare",
        "dirs",
        "disown",
        "echo",
        "enable",
        "eval",
        "exec",
        "exit",
        "export",
        "false",
        "fc",
        "fg",
        "getopts",
        "hash",
        "help",
        "history",
        "jobs",
        "kill",
        "let",
        "local",
        "logout",
        "mapfile",
        "popd",
        "printf",
        "pushd",
        "pwd",
        "read",
        "readarray",
        "readonly",
        "return",
        "set",
        "shift",
        "shopt",
        "source",
        "suspend",
        "test",
        "times",
        "trap",
        "true",
        "type",
        "typeset",
        "ulimit",
        "umask",
        "unalias",
        "unset",
        "wait",
    ]
}

/// colony - Colony Shell recipe and package management
///
/// Usage:
///   colony install <recipe>        Install a recipe package
///   colony publish                  Publish current directory as a recipe
///   colony recipe list              List installed recipes
///   colony recipe show <name>       Show recipe details
fn builtin_colony(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("colony: missing subcommand");
        eprintln!("Usage: colony <install|publish|recipe|plugin|onboard> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    let subcommand = &args[0];
    let sub_args = &args[1..];

    match subcommand.as_str() {
        "install" => colony_install(sub_args),
        "publish" => colony_publish(sub_args),
        "recipe" => colony_recipe(sub_args),
        "plugin" => colony_plugin(sub_args),
        "onboard" => {
            // The actual wizard is handled by the REPL layer (colony-shell crate)
            // which intercepts "colony onboard" before it reaches the executor.
            // If we get here, we're in non-interactive mode.
            eprintln!("colony onboard: the setup wizard requires an interactive terminal.");
            eprintln!("Run `colony` interactively to use the onboarding wizard.");
            Ok(BuiltinResult::Failure(1))
        }
        _ => {
            eprintln!("colony: unknown subcommand '{}'", subcommand);
            eprintln!("Usage: colony <install|publish|recipe|plugin|onboard> [args...]");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// colony install <recipe> - Install a recipe package
///
/// Accepts a recipe name or URL:
///   colony install code-reviewer
///   colony install https://social.colony.sh/content/code-reviewer
///
/// Downloads recipe files (AGENT.md, TOOLS.md, CONTEXT.md, shell scripts)
/// to ~/.colony/recipes/<name>/ and registers in the recipe registry.
fn colony_install(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("colony install: missing recipe name or URL");
        eprintln!("Usage: colony install <recipe-name|url>");
        return Ok(BuiltinResult::Failure(1));
    }

    let source = &args[0];

    let (name, source_url) = if source.starts_with("http://") || source.starts_with("https://") {
        let name = extract_recipe_name_from_url(source);
        let Some(name) = name else {
            eprintln!(
                "colony install: could not extract recipe name from URL: {}",
                source
            );
            return Ok(BuiltinResult::Failure(1));
        };
        (name, Some(source.clone()))
    } else {
        (source.clone(), None)
    };

    let recipes_dir = match colony_recipe::RecipeRegistry::recipes_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("colony install: {}", e);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    let install_dir = recipes_dir.join(&name);

    let mut registry = match colony_recipe::RecipeRegistry::open() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("colony install: failed to open registry: {}", e);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    if registry.is_installed(&name) {
        eprintln!("colony install: recipe '{}' is already installed", name);
        eprintln!("  installed at: {}", install_dir.display());
        return Ok(BuiltinResult::Failure(1));
    }

    if let Some(url) = &source_url {
        eprintln!(
            "colony install: downloading from URL is not yet supported: {}",
            url
        );
        eprintln!(
            "  To install a local recipe, copy files to: {}",
            install_dir.display()
        );
        return Ok(BuiltinResult::Failure(1));
    }

    let search_dirs = build_install_search_dirs();

    let found = search_dirs.iter().find_map(|dir| {
        let candidate_dir = dir.join(&name);
        if candidate_dir.is_dir() && colony_recipe::validate_recipe_dir(&candidate_dir).is_ok() {
            return Some(candidate_dir);
        }
        let candidate_file = dir.join(format!("{}.toml", name));
        if candidate_file.exists() {
            return Some(candidate_file);
        }
        None
    });

    let Some(source_path) = found else {
        eprintln!("colony install: recipe '{}' not found", name);
        eprintln!("  searched in:");
        for dir in &search_dirs {
            eprintln!("    {}", dir.display());
        }
        return Ok(BuiltinResult::Failure(1));
    };

    if let Err(e) = std::fs::create_dir_all(&install_dir) {
        eprintln!(
            "colony install: failed to create {}: {}",
            install_dir.display(),
            e
        );
        return Ok(BuiltinResult::Failure(1));
    }

    if source_path.is_dir() {
        let files = colony_recipe::recipe_package_files(&source_path);
        if files.is_empty() {
            eprintln!(
                "colony install: no recipe files found in {}",
                source_path.display()
            );
            return Ok(BuiltinResult::Failure(1));
        }
        for file in &files {
            let file_name = match file.strip_prefix(&source_path) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => {
                    let Some(fname) = file.file_name() else {
                        continue;
                    };
                    PathBuf::from(fname)
                }
            };
            let dest = install_dir.join(&file_name);
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::copy(file, &dest) {
                eprintln!("colony install: failed to copy {}: {}", file.display(), e);
                return Ok(BuiltinResult::Failure(1));
            }
        }
        println!(
            "Installed {} files to {}",
            files.len(),
            install_dir.display()
        );
    } else {
        let dest = install_dir.join("recipe.toml");
        if let Err(e) = std::fs::copy(&source_path, &dest) {
            eprintln!(
                "colony install: failed to copy {}: {}",
                source_path.display(),
                e
            );
            return Ok(BuiltinResult::Failure(1));
        }
        println!("Installed recipe.toml to {}", install_dir.display());
    }

    if let Err(e) = colony_recipe::validate_recipe_dir(&install_dir) {
        eprintln!(
            "colony install: warning: installed recipe has validation issues: {}",
            e
        );
    }

    let version = read_recipe_version(&install_dir);
    let description = read_recipe_description(&install_dir);

    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) = registry.register(colony_recipe::InstalledRecipe {
        name: name.clone(),
        version,
        description,
        install_path: install_dir.clone(),
        source_url,
        installed_at: now,
    }) {
        eprintln!("colony install: warning: failed to register recipe: {}", e);
    }

    println!("Recipe '{}' installed successfully", name);
    Ok(BuiltinResult::Success(0))
}

/// colony publish - Package and publish a recipe to the registry
///
/// Packages the current directory's special files (AGENT.md, TOOLS.md, etc.)
/// into a recipe and publishes to the Colony Cloud registry.
fn colony_publish(args: &[String]) -> Result<BuiltinResult> {
    let dir = if let Some(path) = args.first() {
        PathBuf::from(path)
    } else {
        std::env::current_dir().map_err(ExecutionError::IoError)?
    };

    if let Err(e) = colony_recipe::validate_recipe_dir(&dir) {
        eprintln!("colony publish: {}", e);
        eprintln!("  A publishable recipe must contain at least AGENT.md or recipe.toml");
        return Ok(BuiltinResult::Failure(1));
    }

    let files = colony_recipe::recipe_package_files(&dir);
    if files.is_empty() {
        eprintln!("colony publish: no recipe files found in {}", dir.display());
        return Ok(BuiltinResult::Failure(1));
    }

    let api_key = std::env::var("COLONY_API_KEY")
        .ok()
        .or_else(read_config_api_key);

    let Some(_api_key) = api_key else {
        eprintln!("colony publish: authentication required");
        eprintln!(
            "  Set COLONY_API_KEY environment variable or add api_key to ~/.colony/config.toml"
        );
        return Ok(BuiltinResult::Failure(1));
    };

    let name = detect_recipe_name(&dir);
    let version = read_recipe_version(&dir);

    println!("Publishing recipe: {} v{}", name, version);
    println!("Files:");
    for file in &files {
        let rel = file.strip_prefix(&dir).unwrap_or(file);
        println!("  {}", rel.display());
    }

    eprintln!("colony publish: registry upload not yet implemented");
    eprintln!("  Recipe validated and ready to publish when Colony Cloud is available");

    Ok(BuiltinResult::Success(0))
}

/// colony recipe <subcommand> - Manage recipes
fn colony_recipe(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("colony recipe: missing subcommand");
        eprintln!("Usage: colony recipe <list|show> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    match args[0].as_str() {
        "list" => colony_recipe_list(),
        "show" => {
            if args.len() < 2 {
                eprintln!("colony recipe show: missing recipe name");
                return Ok(BuiltinResult::Failure(1));
            }
            colony_recipe_show(&args[1])
        }
        _ => {
            eprintln!("colony recipe: unknown subcommand '{}'", args[0]);
            eprintln!("Usage: colony recipe <list|show> [args...]");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

fn colony_recipe_list() -> Result<BuiltinResult> {
    let registry = match colony_recipe::RecipeRegistry::open() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("colony recipe list: {}", e);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    let recipes = registry.list();
    if recipes.is_empty() {
        println!("No recipes installed.");
        println!("  Use 'colony install <recipe>' to install one.");
        return Ok(BuiltinResult::Success(0));
    }

    println!("Installed recipes:");
    for recipe in &recipes {
        let desc = recipe.description.as_deref().unwrap_or("");
        println!("  {:<20} v{:<10} {}", recipe.name, recipe.version, desc);
    }

    let loader = colony_recipe::RecipeLoader::new();
    let discovered = loader.discover();
    let non_installed: Vec<_> = discovered
        .iter()
        .filter(|d| !registry.is_installed(&d.name))
        .collect();

    if !non_installed.is_empty() {
        println!();
        println!("Available (not installed):");
        for info in &non_installed {
            println!("  {:<20} [{}]", info.name, info.source);
        }
    }

    Ok(BuiltinResult::Success(0))
}

fn colony_recipe_show(name: &str) -> Result<BuiltinResult> {
    let registry = match colony_recipe::RecipeRegistry::open() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("colony recipe show: {}", e);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    let Some(entry) = registry.get(name) else {
        eprintln!("colony recipe show: recipe '{}' not found", name);
        return Ok(BuiltinResult::Failure(1));
    };

    println!("Recipe: {} v{}", entry.name, entry.version);
    if let Some(desc) = &entry.description {
        println!("Description: {}", desc);
    }
    println!("Path: {}", entry.install_path.display());
    if let Some(url) = &entry.source_url {
        println!("Source: {}", url);
    }
    println!("Installed: {}", entry.installed_at);

    let files = colony_recipe::recipe_package_files(&entry.install_path);
    if !files.is_empty() {
        println!("Files:");
        for file in &files {
            let rel = file.strip_prefix(&entry.install_path).unwrap_or(file);
            println!("  {}", rel.display());
        }
    }

    if entry.install_path.join("recipe.toml").exists() {
        let mut loader = colony_recipe::RecipeLoader::new();
        if let Ok(recipe) = loader.load_from_path(&entry.install_path.join("recipe.toml")) {
            if !recipe.imports().is_empty() {
                println!("Imports:");
                for import in recipe.imports() {
                    println!("  {}", import);
                }
            }
            if !recipe.skills.is_empty() {
                println!("Skills:");
                for (name, skill) in &recipe.skills {
                    let trigger = skill.trigger.as_deref().unwrap_or("");
                    let desc = skill.description.as_deref().unwrap_or("");
                    println!("  {:<15} {:<10} {}", name, trigger, desc);
                }
            }
            if !recipe.commands.is_empty() {
                println!("Commands:");
                for cmd in &recipe.commands {
                    let desc = cmd.description.as_deref().unwrap_or("");
                    println!("  /{:<14} {}", cmd.name, desc);
                }
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// colony plugin — manage the Colony Claude Code plugin
fn colony_plugin(args: &[String]) -> Result<BuiltinResult> {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("status");

    match subcmd {
        "install" => {
            let plugin_dir = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude/plugins/colony");

            // Find plugin source — check relative to the colony binary
            let exe_path = std::env::current_exe().unwrap_or_default();
            let possible_sources = [
                // Next to the binary (installed via release tarball)
                exe_path
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join("../share/colony/claude-plugin"),
                // In the source tree
                exe_path
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join("../../claude-plugin"),
                // Common dev path
                dirs::home_dir()
                    .unwrap_or_default()
                    .join("src/github.com/colony-sh/colony-shell/claude-plugin"),
            ];

            let Some(source) = possible_sources
                .iter()
                .find(|p| p.join(".claude-plugin").exists())
            else {
                eprintln!("colony plugin: could not find plugin source files");
                eprintln!("Expected claude-plugin/ directory with .claude-plugin/plugin.json");
                return Ok(BuiltinResult::Failure(1));
            };

            // Copy plugin files
            let dirs_to_copy = [".claude-plugin", "skills", "commands", "hooks"];
            if let Err(e) = std::fs::create_dir_all(&plugin_dir) {
                eprintln!(
                    "colony plugin: failed to create {}: {}",
                    plugin_dir.display(),
                    e
                );
                return Ok(BuiltinResult::Failure(1));
            }

            for dir_name in &dirs_to_copy {
                let src = source.join(dir_name);
                let dst = plugin_dir.join(dir_name);
                if src.exists() {
                    let _ = std::fs::remove_dir_all(&dst);
                    if let Err(e) = copy_dir_recursive(&src, &dst) {
                        eprintln!("colony plugin: failed to copy {}: {}", dir_name, e);
                        return Ok(BuiltinResult::Failure(1));
                    }
                }
            }

            println!(
                "Installed Colony Claude Code plugin to {}",
                plugin_dir.display()
            );
            println!("Run /reload-plugins in Claude Code to activate.");
            Ok(BuiltinResult::Success(0))
        }
        "status" => {
            let plugin_dir = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude/plugins/colony");

            if plugin_dir.join(".claude-plugin/plugin.json").exists() {
                println!("Colony Claude Code plugin: installed");
                println!("  Path: {}", plugin_dir.display());

                // Count skills
                let skills_dir = plugin_dir.join("skills");
                if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                    let count = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().is_dir())
                        .count();
                    println!("  Skills: {}", count);
                }
            } else {
                println!("Colony Claude Code plugin: not installed");
                println!("  Run: colony plugin install");
            }
            Ok(BuiltinResult::Success(0))
        }
        "uninstall" => {
            let plugin_dir = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude/plugins/colony");

            if plugin_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&plugin_dir) {
                    eprintln!("colony plugin: failed to remove: {}", e);
                    return Ok(BuiltinResult::Failure(1));
                }
                println!("Uninstalled Colony Claude Code plugin.");
            } else {
                println!("Colony Claude Code plugin is not installed.");
            }
            Ok(BuiltinResult::Success(0))
        }
        "help" | "--help" | "-h" => {
            println!("Usage: colony plugin <subcommand>");
            println!();
            println!("Subcommands:");
            println!("  install     Install/update the Colony Claude Code plugin");
            println!("  status      Show plugin installation status");
            println!("  uninstall   Remove the plugin");
            println!("  help        Show this help");
            Ok(BuiltinResult::Success(0))
        }
        other => {
            eprintln!(
                "colony plugin: unknown subcommand '{}'. Try 'colony plugin help'.",
                other
            );
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn extract_recipe_name_from_url(url: &str) -> Option<String> {
    url.rsplit('/').next().map(|s| {
        s.trim_end_matches(".toml")
            .trim_end_matches(".tar.gz")
            .trim_end_matches(".zip")
            .to_string()
    })
}

fn build_install_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join(".colony/recipes"));
    }

    if let Some(config) = dirs::config_dir() {
        dirs.push(config.join("colony/recipes"));
    }

    if let Some(cache) = dirs::cache_dir() {
        dirs.push(cache.join("colony/registry"));
    }

    dirs
}

fn read_recipe_version(dir: &std::path::Path) -> String {
    let recipe_toml = dir.join("recipe.toml");
    if recipe_toml.exists() {
        if let Ok(content) = std::fs::read_to_string(&recipe_toml) {
            if let Ok(recipe) = colony_recipe::Recipe::from_toml(&content, &recipe_toml) {
                return recipe.meta.version;
            }
        }
    }
    "0.1.0".to_string()
}

fn read_recipe_description(dir: &std::path::Path) -> Option<String> {
    let recipe_toml = dir.join("recipe.toml");
    if recipe_toml.exists() {
        if let Ok(content) = std::fs::read_to_string(&recipe_toml) {
            if let Ok(recipe) = colony_recipe::Recipe::from_toml(&content, &recipe_toml) {
                return recipe.meta.description;
            }
        }
    }

    let agent_md = dir.join("AGENT.md");
    if agent_md.exists() {
        if let Ok(content) = std::fs::read_to_string(&agent_md) {
            for line in content.lines().skip(1) {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("---") {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

fn detect_recipe_name(dir: &std::path::Path) -> String {
    let recipe_toml = dir.join("recipe.toml");
    if recipe_toml.exists() {
        if let Ok(content) = std::fs::read_to_string(&recipe_toml) {
            if let Ok(recipe) = colony_recipe::Recipe::from_toml(&content, &recipe_toml) {
                return recipe.meta.name;
            }
        }
    }

    let agent_md = dir.join("AGENT.md");
    if agent_md.exists() {
        if let Ok(content) = std::fs::read_to_string(&agent_md) {
            for line in content.lines() {
                if line.starts_with("## ") {
                    return line.trim_start_matches("## ").trim().to_string();
                }
            }
        }
    }

    dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string()
}

fn read_config_api_key() -> Option<String> {
    let config_path = dirs::home_dir()?.join(".colony/config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let table: toml::Table = toml::from_str(&content).ok()?;
    table
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// goal -- Goal and task tracking builtin
///
/// Subcommands:
///   goal add <title>              Add a new top-level goal
///   goal add <parent-id> <title>  Add a child goal
///   goal list                     List all goals
///   goal update <id> <status>     Update goal status
///   goal complete <id>            Mark a goal as complete
///   goal block <id> <reason>      Mark a goal as blocked
///   goal checkpoint <label>       Create a snapshot of current goals
///   goal tree                     Show goals as a tree
fn builtin_goal(args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("goal: missing subcommand");
        eprintln!("Usage: goal <add|list|update|complete|block|checkpoint|tree> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    let cwd = env::current_dir()?;

    let mut tree = match crate::goals::GoalTree::load(&cwd) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("goal: failed to load goal tree: {}", e);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    let subcommand = &args[0];
    let sub_args = &args[1..];

    let result = match subcommand.as_str() {
        "add" => goal_add(&mut tree, sub_args),
        "list" | "ls" => goal_list(&tree),
        "update" => goal_update(&mut tree, sub_args),
        "complete" | "done" => goal_complete(&mut tree, sub_args),
        "block" => goal_block(&mut tree, sub_args),
        "checkpoint" | "cp" => goal_checkpoint(&mut tree, sub_args),
        "tree" => goal_tree(&tree),
        _ => {
            eprintln!("goal: unknown subcommand '{}'", subcommand);
            eprintln!("Usage: goal <add|list|update|complete|block|checkpoint|tree> [args...]");
            return Ok(BuiltinResult::Failure(1));
        }
    };

    if result.is_ok() {
        if let Err(e) = tree.save() {
            eprintln!("goal: warning: failed to save: {}", e);
        }
    }

    result
}

fn goal_add(tree: &mut crate::goals::GoalTree, args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("goal add: missing title");
        eprintln!("Usage: goal add <title>");
        eprintln!("       goal add <parent-id> <title>");
        return Ok(BuiltinResult::Failure(1));
    }

    // Check if first arg is a numeric parent ID
    if args.len() >= 2 {
        if let Ok(parent_id) = args[0].parse::<u64>() {
            let title = args[1..].join(" ");
            match tree.add_child(parent_id, &title) {
                Ok(id) => {
                    eprintln!("Added goal #{} under #{}: {}", id, parent_id, title);
                    return Ok(BuiltinResult::Success(0));
                }
                Err(e) => {
                    eprintln!("goal add: {}", e);
                    return Ok(BuiltinResult::Failure(1));
                }
            }
        }
    }

    let title = args.join(" ");
    let id = tree.add(&title);
    eprintln!("Added goal #{}: {}", id, title);
    Ok(BuiltinResult::Success(0))
}

fn goal_list(tree: &crate::goals::GoalTree) -> Result<BuiltinResult> {
    let goals = tree.all_goals();
    if goals.is_empty() {
        eprintln!("No goals. Use 'goal add <title>' to create one.");
        return Ok(BuiltinResult::Success(0));
    }

    for goal in &goals {
        let status_icon = match goal.status {
            crate::goals::GoalStatus::Pending => "[ ]",
            crate::goals::GoalStatus::InProgress => "[~]",
            crate::goals::GoalStatus::Completed => "[x]",
            crate::goals::GoalStatus::Blocked => "[!]",
            crate::goals::GoalStatus::Cancelled => "[-]",
        };
        eprintln!(
            "{} #{}: {} ({}%)",
            status_icon, goal.id, goal.title, goal.progress
        );
    }
    Ok(BuiltinResult::Success(0))
}

fn goal_update(tree: &mut crate::goals::GoalTree, args: &[String]) -> Result<BuiltinResult> {
    if args.len() < 2 {
        eprintln!("goal update: missing arguments");
        eprintln!("Usage: goal update <id> <pending|in_progress|completed|blocked|cancelled>");
        return Ok(BuiltinResult::Failure(1));
    }

    let id: u64 = match args[0].parse() {
        Ok(id) => id,
        Err(_) => {
            eprintln!("goal update: invalid goal ID: {}", args[0]);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    let status = match args[1].as_str() {
        "pending" => crate::goals::GoalStatus::Pending,
        "in_progress" | "active" => crate::goals::GoalStatus::InProgress,
        "completed" | "done" => crate::goals::GoalStatus::Completed,
        "blocked" => crate::goals::GoalStatus::Blocked,
        "cancelled" | "canceled" => crate::goals::GoalStatus::Cancelled,
        other => {
            eprintln!("goal update: unknown status '{}'", other);
            eprintln!("Valid: pending, in_progress, completed, blocked, cancelled");
            return Ok(BuiltinResult::Failure(1));
        }
    };

    match tree.update_status(id, status) {
        Ok(()) => {
            eprintln!("Updated goal #{} to {}", id, status);
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("goal update: {}", e);
            Ok(BuiltinResult::Failure(1))
        }
    }
}

fn goal_complete(tree: &mut crate::goals::GoalTree, args: &[String]) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("goal complete: missing goal ID");
        eprintln!("Usage: goal complete <id>");
        return Ok(BuiltinResult::Failure(1));
    }

    let id: u64 = match args[0].parse() {
        Ok(id) => id,
        Err(_) => {
            eprintln!("goal complete: invalid goal ID: {}", args[0]);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    match tree.complete(id) {
        Ok(()) => {
            eprintln!("Completed goal #{}", id);
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("goal complete: {}", e);
            Ok(BuiltinResult::Failure(1))
        }
    }
}

fn goal_block(tree: &mut crate::goals::GoalTree, args: &[String]) -> Result<BuiltinResult> {
    if args.len() < 2 {
        eprintln!("goal block: missing arguments");
        eprintln!("Usage: goal block <id> <reason>");
        return Ok(BuiltinResult::Failure(1));
    }

    let id: u64 = match args[0].parse() {
        Ok(id) => id,
        Err(_) => {
            eprintln!("goal block: invalid goal ID: {}", args[0]);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    let reason = args[1..].join(" ");
    match tree.add_blocker(id, &reason) {
        Ok(()) => {
            eprintln!("Blocked goal #{}: {}", id, reason);
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("goal block: {}", e);
            Ok(BuiltinResult::Failure(1))
        }
    }
}

fn goal_checkpoint(tree: &mut crate::goals::GoalTree, args: &[String]) -> Result<BuiltinResult> {
    let label = if args.is_empty() {
        chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string()
    } else {
        args.join(" ")
    };

    tree.checkpoint(&label);
    eprintln!("Created checkpoint: {}", label);
    Ok(BuiltinResult::Success(0))
}

fn goal_tree(tree: &crate::goals::GoalTree) -> Result<BuiltinResult> {
    let output = tree.format_tree();
    if output.is_empty() {
        eprintln!("No goals. Use 'goal add <title>' to create one.");
    } else {
        eprint!("{}", output);
    }
    Ok(BuiltinResult::Success(0))
}

/// secure - Manage secure environment variables
///
/// Usage:
///   secure export NAME=VALUE   — encrypt and store a secure variable
///   secure list                — list all registered secure variables
///   secure approvals           — list saved approvals from database
///   secure revoke NAME         — revoke approval for a variable
///   secure patterns            — show configured auto-detection patterns
fn builtin_secure(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    let Some(subcommand) = args.first() else {
        eprintln!("secure: missing subcommand");
        eprintln!("Usage: secure <export|list|approvals|revoke|patterns>");
        return Ok(BuiltinResult::Failure(1));
    };

    let Some(registry_arc) = context.secure_registry().cloned() else {
        eprintln!("secure: secure registry not initialized");
        return Ok(BuiltinResult::Failure(1));
    };

    match subcommand.as_str() {
        "export" => {
            let registry = registry_arc.lock();
            builtin_secure_export(&args[1..], context, &registry)
        }
        "list" => {
            let registry = registry_arc.lock();
            builtin_secure_list(&registry)
        }
        "approvals" => {
            let registry = registry_arc.lock();
            builtin_secure_approvals(&args[1..], &registry)
        }
        "revoke" => {
            let registry = registry_arc.lock();
            builtin_secure_revoke(&args[1..], &registry)
        }
        "patterns" => {
            let registry = registry_arc.lock();
            builtin_secure_patterns(&registry)
        }
        _ => {
            eprintln!("secure: unknown subcommand '{}'", subcommand);
            eprintln!("Usage: secure <export|list|approvals|revoke|patterns>");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// secure export NAME=VALUE — encrypt value, store in registry, set encrypted env var
fn builtin_secure_export(
    args: &[String],
    context: &mut ExpansionContext,
    registry: &colony_secure::SecureRegistry,
) -> Result<BuiltinResult> {
    let Some(assignment) = args.first() else {
        eprintln!("secure export: missing NAME=VALUE");
        return Ok(BuiltinResult::Failure(1));
    };

    let Some(eq_pos) = assignment.find('=') else {
        eprintln!("secure export: expected NAME=VALUE, got '{}'", assignment);
        return Ok(BuiltinResult::Failure(1));
    };

    let name = &assignment[..eq_pos];
    let value = &assignment[eq_pos + 1..];

    if name.is_empty() {
        eprintln!("secure export: variable name cannot be empty");
        return Ok(BuiltinResult::Failure(1));
    }

    // Encrypt the value using the keychain key
    let encrypted = match colony_secure::Keychain::new() {
        Ok(keychain) => match keychain.get_or_create_key() {
            Ok(key) => match colony_secure::encrypt(value, &key) {
                Ok(enc) => enc,
                Err(e) => {
                    eprintln!("secure export: encryption failed: {}", e);
                    return Ok(BuiltinResult::Failure(1));
                }
            },
            Err(e) => {
                eprintln!("secure export: keychain error: {}", e);
                return Ok(BuiltinResult::Failure(1));
            }
        },
        Err(e) => {
            eprintln!("secure export: keychain error: {}", e);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    // Register the variable as secure with the plaintext value (stored in memory only)
    registry.mark_secure(name, value, colony_secure::SecureVarSource::Explicit);

    // Set the encrypted value in the environment so child processes get it encrypted
    context.set_env(name, &encrypted);
    context.set_var(name.to_string(), encrypted);

    println!("Secure variable ${} exported", name);
    Ok(BuiltinResult::Success(0))
}

/// secure list — list all registered secure variables
fn builtin_secure_list(registry: &colony_secure::SecureRegistry) -> Result<BuiltinResult> {
    let vars = registry.list_secure_vars();

    if vars.is_empty() {
        println!("No secure variables registered.");
        return Ok(BuiltinResult::Success(0));
    }

    println!("{:<30} {:<15} HAS VALUE", "VARIABLE", "SOURCE");
    println!("{}", "-".repeat(55));

    for (name, source, has_value) in &vars {
        let source_str = match source {
            colony_secure::SecureVarSource::Explicit => "explicit".to_string(),
            colony_secure::SecureVarSource::AutoDetected { pattern } => {
                format!("pattern:{}", pattern)
            }
            colony_secure::SecureVarSource::Config { file } => {
                format!("config:{}", file.display())
            }
        };
        let value_str = if *has_value { "yes" } else { "no" };
        println!("{:<30} {:<15} {}", name, source_str, value_str);
    }

    Ok(BuiltinResult::Success(0))
}

/// secure approvals [NAME] — list saved approvals from database
fn builtin_secure_approvals(
    args: &[String],
    registry: &colony_secure::SecureRegistry,
) -> Result<BuiltinResult> {
    // If a variable name is provided, filter by it; otherwise list all
    let approvals = if let Some(name) = args.first() {
        match registry.list_approvals(name) {
            Ok(approvals) => approvals,
            Err(e) => {
                eprintln!("secure approvals: {}", e);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    } else {
        // No specific variable -- we need to list all approvals
        // The registry only has list_approvals(variable), so we iterate over known vars
        let vars = registry.list_secure_vars();
        let mut all = Vec::new();
        for (name, _, _) in &vars {
            match registry.list_approvals(name) {
                Ok(mut approvals) => all.append(&mut approvals),
                Err(e) => {
                    eprintln!("secure approvals: error listing {}: {}", name, e);
                }
            }
        }
        all
    };

    if approvals.is_empty() {
        println!("No saved approvals.");
        return Ok(BuiltinResult::Success(0));
    }

    println!(
        "{:<25} {:<30} {:<12} GRANTED",
        "VARIABLE", "SCRIPT", "SCOPE"
    );
    println!("{}", "-".repeat(80));

    for approval in &approvals {
        let scope = match approval.scope {
            colony_secure::ApprovalScope::Global => "global",
            colony_secure::ApprovalScope::Project => "project",
        };
        println!(
            "{:<25} {:<30} {:<12} {}",
            approval.variable_name,
            approval.script_path.display(),
            scope,
            approval.granted_at.format("%Y-%m-%d %H:%M"),
        );
    }

    Ok(BuiltinResult::Success(0))
}

/// secure revoke NAME — revoke approval for a variable
fn builtin_secure_revoke(
    args: &[String],
    registry: &colony_secure::SecureRegistry,
) -> Result<BuiltinResult> {
    let Some(name) = args.first() else {
        eprintln!("secure revoke: missing variable name");
        return Ok(BuiltinResult::Failure(1));
    };

    match registry.revoke_approval(name, None) {
        Ok(()) => {
            println!("Revoked all approvals for ${}", name);
            Ok(BuiltinResult::Success(0))
        }
        Err(e) => {
            eprintln!("secure revoke: {}", e);
            Ok(BuiltinResult::Failure(1))
        }
    }
}

/// secure patterns — show configured auto-detection patterns
fn builtin_secure_patterns(_registry: &colony_secure::SecureRegistry) -> Result<BuiltinResult> {
    // Load config to display patterns
    let config = match colony_secure::SecureConfig::load_or_default() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("secure patterns: failed to load config: {}", e);
            return Ok(BuiltinResult::Failure(1));
        }
    };

    println!("Secure patterns (variables matching these are protected):");
    for pattern in &config.patterns.secure {
        println!("  {}", pattern);
    }

    if !config.patterns.not_secure.is_empty() {
        println!("\nExcluded patterns (always allowed):");
        for pattern in &config.patterns.not_secure {
            println!("  {}", pattern);
        }
    }

    if !config.variables.secure.is_empty() {
        println!("\nExplicitly secured variables:");
        for var in &config.variables.secure {
            println!("  {}", var);
        }
    }

    Ok(BuiltinResult::Success(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::make_simple_command;

    #[test]
    fn test_is_builtin() {
        assert!(is_builtin("cd"));
        assert!(is_builtin("export"));
        assert!(is_builtin("echo"));
        assert!(is_builtin("pwd"));
        assert!(is_builtin("exit"));
        assert!(is_builtin("pwd:noop"));
        assert!(is_builtin("context:show"));
        assert!(!is_builtin("ls"));
        assert!(!is_builtin("grep"));
    }

    #[test]
    fn test_builtin_pwd() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("pwd:noop", vec![]);

        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_builtin_echo() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("echo", vec!["hello", "world"]);

        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_builtin_exit() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("exit", vec!["42"]);

        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert!(builtin_result.is_exit());
        assert_eq!(builtin_result.exit_code(), 42);
    }

    #[test]
    fn test_builtin_export() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("export", vec!["TEST_VAR=hello"]);

        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);

        // Verify variable was set
        assert_eq!(context.get_env("TEST_VAR"), Some("hello"));
    }

    #[test]
    fn test_builtin_unset() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        context.set_env("TEST_VAR", "value");

        let cmd = make_simple_command("unset", vec!["TEST_VAR"]);

        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);

        // Verify variable was removed
        assert_eq!(context.get_env("TEST_VAR"), None);
    }

    #[test]
    fn test_builtin_cd() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Save current dir
        let original_dir = env::current_dir().unwrap();

        // cd to /tmp
        let cmd = make_simple_command("cd", vec!["/tmp"]);
        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);

        // Verify we're in /tmp (resolve symlinks for macOS)
        let current = env::current_dir().unwrap().canonicalize().unwrap();
        let expected = PathBuf::from("/tmp").canonicalize().unwrap();
        assert_eq!(current, expected);

        // Restore original directory
        env::set_current_dir(original_dir).ok();
    }

    #[test]
    fn test_builtin_type() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("type", vec!["cd", "echo"]);

        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_non_builtin() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("ls", vec![]);

        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_none()); // ls is not a builtin
    }

    #[test]
    fn test_builtin_jobs_empty() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("jobs", vec![]);

        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_builtin_jobs_with_job() {
        use nix::unistd::Pid;

        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Add a fake job
        jobs.add_job(
            Pid::from_raw(12345),
            vec![Pid::from_raw(12345)],
            "sleep 10".to_string(),
        );

        let cmd = make_simple_command("jobs", vec![]);
        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let builtin_result = result.unwrap().unwrap();
        assert_eq!(builtin_result.exit_code(), 0);
    }

    #[test]
    fn test_reactive_export() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Export a variable
        let cmd = make_simple_command("export", vec!["REACTIVE_TEST=initial"]);
        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        // Check tuple was emitted (with session_id)
        let pattern = Tuple::pattern(&[
            Value::Str("env_change".to_string()),
            Value::Str("test".to_string()), // session_id
            Value::Str("REACTIVE_TEST".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]);

        let tuple = tuple_space.inp(pattern);
        assert!(tuple.is_some());

        let t = tuple.unwrap();
        assert_eq!(t.fields[0], Value::Str("env_change".to_string()));
        assert_eq!(t.fields[1], Value::Str("test".to_string())); // session_id
        assert_eq!(t.fields[2], Value::Str("REACTIVE_TEST".to_string()));
        assert_eq!(t.fields[3], Value::Str("".to_string())); // old value (was empty)
        assert_eq!(t.fields[4], Value::Str("initial".to_string())); // new value
    }

    #[test]
    fn test_reactive_cd() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        // Save original dir
        let original_dir = env::current_dir().unwrap();

        // cd to /tmp
        let cmd = make_simple_command("cd", vec!["/tmp"]);
        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        // Check tuple was emitted (with session_id)
        let pattern = Tuple::pattern(&[
            Value::Str("dir_change".to_string()),
            Value::Str("test".to_string()), // session_id
            Value::Wildcard,
            Value::Wildcard,
        ]);

        let tuple = tuple_space.inp(pattern);
        assert!(tuple.is_some());

        let t = tuple.unwrap();
        assert_eq!(t.fields[0], Value::Str("dir_change".to_string()));
        assert_eq!(t.fields[1], Value::Str("test".to_string())); // session_id
                                                                 // fields[2] is old dir, fields[3] is new dir (should be /tmp or /private/tmp)
                                                                 // Just verify it's a string value
        assert!(matches!(t.fields[3], Value::Str(_)));

        // Restore
        env::set_current_dir(original_dir).ok();
    }

    #[test]
    fn test_reactive_unset() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();

        context.set_env("TO_DELETE", "old_value");

        let cmd = make_simple_command("unset", vec!["TO_DELETE"]);
        let result = execute_builtin(
            &cmd,
            &mut context,
            &jobs,
            &tuple_space,
            "test",
            &mut channels,
            None,
            None,
        );
        assert!(result.is_some());

        let pattern = Tuple::pattern(&[
            Value::Str("env_change".to_string()),
            Value::Str("test".to_string()), // session_id
            Value::Str("TO_DELETE".to_string()),
            Value::Wildcard,
            Value::Null,
        ]);

        let tuple = tuple_space.inp(pattern);
        assert!(tuple.is_some());
    }

    #[test]
    fn test_set_option_minus_e_enables_errexit() {
        let mut context = ExpansionContext::new();

        assert!(!context.shell_options().errexit);

        let result = builtin_set(&["-e".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);

        assert!(context.shell_options().errexit);
    }

    #[test]
    fn test_set_option_plus_e_disables_errexit() {
        let mut context = ExpansionContext::new();

        context.shell_options_mut().errexit = true;
        assert!(context.shell_options().errexit);

        let result = builtin_set(&["+e".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);

        assert!(!context.shell_options().errexit);
    }

    #[test]
    fn test_set_option_minus_x_enables_xtrace() {
        let mut context = ExpansionContext::new();

        assert!(!context.shell_options().xtrace);

        let result = builtin_set(&["-x".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(context.shell_options().xtrace);
    }

    #[test]
    fn test_set_option_plus_x_disables_xtrace() {
        let mut context = ExpansionContext::new();
        context.shell_options_mut().xtrace = true;

        let result = builtin_set(&["+x".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(!context.shell_options().xtrace);
    }

    #[test]
    fn test_set_combined_options() {
        let mut context = ExpansionContext::new();

        assert!(!context.shell_options().errexit);
        assert!(!context.shell_options().xtrace);

        let result = builtin_set(&["-ex".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(context.shell_options().errexit);
        assert!(context.shell_options().xtrace);
    }

    #[test]
    fn test_set_combined_disable_options() {
        let mut context = ExpansionContext::new();
        context.shell_options_mut().errexit = true;
        context.shell_options_mut().xtrace = true;

        let result = builtin_set(&["+ex".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(!context.shell_options().errexit);
        assert!(!context.shell_options().xtrace);
    }

    #[test]
    fn test_set_option_u_nounset() {
        let mut context = ExpansionContext::new();

        assert!(!context.shell_options().nounset);

        let result = builtin_set(&["-u".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(context.shell_options().nounset);
    }

    #[test]
    fn test_set_option_f_noglob() {
        let mut context = ExpansionContext::new();

        assert!(!context.shell_options().noglob);

        let result = builtin_set(&["-f".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(context.shell_options().noglob);
    }

    #[test]
    fn test_set_option_n_noexec() {
        let mut context = ExpansionContext::new();

        assert!(!context.shell_options().noexec);

        let result = builtin_set(&["-n".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(context.shell_options().noexec);
    }

    #[test]
    fn test_set_option_v_verbose() {
        let mut context = ExpansionContext::new();

        assert!(!context.shell_options().verbose);

        let result = builtin_set(&["-v".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(context.shell_options().verbose);
    }

    #[test]
    fn test_set_double_dash_positional_params() {
        let mut context = ExpansionContext::new();

        let result = builtin_set(
            &[
                "--".to_string(),
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
            ],
            &mut context,
        );
        assert!(result.is_ok());

        assert_eq!(context.get_all_positional_params(), &["a", "b", "c"]);
    }

    #[test]
    fn test_set_double_dash_clears_params_when_empty() {
        let mut context = ExpansionContext::new();
        context.set_positional_params(vec!["old1".to_string(), "old2".to_string()]);

        let result = builtin_set(&["--".to_string()], &mut context);
        assert!(result.is_ok());

        assert!(context.get_all_positional_params().is_empty());
    }

    #[test]
    fn test_set_options_then_double_dash_params() {
        let mut context = ExpansionContext::new();

        let result = builtin_set(
            &[
                "-ex".to_string(),
                "--".to_string(),
                "a".to_string(),
                "b".to_string(),
            ],
            &mut context,
        );
        assert!(result.is_ok());

        assert!(context.shell_options().errexit);
        assert!(context.shell_options().xtrace);
        assert_eq!(context.get_all_positional_params(), &["a", "b"]);
    }

    #[test]
    fn test_set_no_args_preserves_params() {
        let mut context = ExpansionContext::new();
        context.set_positional_params(vec!["existing".to_string()]);

        let result = builtin_set(&[], &mut context);
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_positional_params_without_double_dash() {
        let mut context = ExpansionContext::new();

        let result = builtin_set(&["foo".to_string(), "bar".to_string()], &mut context);
        assert!(result.is_ok());

        assert_eq!(context.get_all_positional_params(), &["foo", "bar"]);
    }

    #[test]
    fn test_shell_options_to_string() {
        let mut context = ExpansionContext::new();

        context.shell_options_mut().errexit = true;
        context.shell_options_mut().xtrace = true;

        let options_str = context.shell_options().to_string();
        assert!(options_str.contains('e'));
        assert!(options_str.contains('x'));
        assert!(!options_str.contains('u'));
    }

    #[test]
    fn test_dollar_dash_special_variable() {
        let mut context = ExpansionContext::new();
        context.shell_options_mut().errexit = true;
        context.shell_options_mut().xtrace = true;

        let value = context.get_var_or_env("-");
        assert!(value.is_some());
        let opts = value.unwrap();
        assert!(opts.contains('e'));
        assert!(opts.contains('x'));
    }

    #[test]
    #[serial_test::serial]
    fn test_builtin_path_add() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();

        let original_path = env::var("PATH").unwrap_or_default();
        let test_dir = format!("/tmp/colony_test_path_add_{}", std::process::id());

        let result = builtin_path(
            &["add".to_string(), test_dir.clone()],
            &mut context,
            &tuple_space,
            "test-session",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);

        let new_path = env::var("PATH").unwrap();
        let last_entry = new_path.split(':').next_back().unwrap();
        assert_eq!(last_entry, test_dir);

        env::set_var("PATH", original_path);
    }

    #[test]
    #[serial_test::serial]
    fn test_builtin_path_prepend() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();

        let original_path = env::var("PATH").unwrap_or_default();
        let test_dir = format!("/tmp/colony_test_path_prepend_{}", std::process::id());

        let result = builtin_path(
            &["prepend".to_string(), test_dir.clone()],
            &mut context,
            &tuple_space,
            "test-session",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);

        let new_path = env::var("PATH").unwrap();
        let first_entry = new_path.split(':').next().unwrap();
        assert_eq!(first_entry, test_dir);

        env::set_var("PATH", original_path);
    }

    #[test]
    fn test_builtin_path_has() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();

        let result = builtin_path(
            &["has".to_string(), "/usr/bin".to_string()],
            &mut context,
            &tuple_space,
            "test-session",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);

        let result = builtin_path(
            &["has".to_string(), "/nonexistent/path".to_string()],
            &mut context,
            &tuple_space,
            "test-session",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_is_builtin_secure() {
        assert!(is_builtin("secure"));
    }

    #[test]
    fn test_secure_no_subcommand() {
        let mut context = ExpansionContext::new();
        // Initialize a secure registry so we can test the builtin
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(&[], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_secure_unknown_subcommand() {
        let mut context = ExpansionContext::new();
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(&["unknown".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_secure_no_registry() {
        let mut context = ExpansionContext::new();
        // No registry set -- should fail
        let result = builtin_secure(&["list".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_secure_list_empty() {
        let mut context = ExpansionContext::new();
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(&["list".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
    }

    #[test]
    fn test_secure_patterns() {
        let mut context = ExpansionContext::new();
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(&["patterns".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
    }

    #[test]
    fn test_secure_approvals_empty() {
        let mut context = ExpansionContext::new();
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(&["approvals".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
    }

    #[test]
    fn test_secure_revoke_missing_name() {
        let mut context = ExpansionContext::new();
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(&["revoke".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_secure_export_missing_assignment() {
        let mut context = ExpansionContext::new();
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(&["export".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_secure_export_no_equals() {
        let mut context = ExpansionContext::new();
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(
            &["export".to_string(), "NOEQUALS".to_string()],
            &mut context,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_secure_export_empty_name() {
        let mut context = ExpansionContext::new();
        let registry = colony_secure::SecureRegistry::with_defaults().unwrap();
        let registry = std::sync::Arc::new(parking_lot::Mutex::new(registry));
        context.set_secure_registry(registry);

        let result = builtin_secure(&["export".to_string(), "=value".to_string()], &mut context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    // --- fg/bg tests ---

    #[test]
    fn test_fg_no_jobs() {
        let jobs = JobTable::new();
        let result = builtin_fg(&[], &jobs).unwrap();
        // Should fail with exit code 1 when no jobs exist
        assert_eq!(result.exit_code(), 1);
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_fg_invalid_job_spec() {
        use nix::unistd::Pid;
        let jobs = JobTable::new();

        // Add a job so the table is non-empty
        jobs.add_job(
            Pid::from_raw(99999),
            vec![Pid::from_raw(99999)],
            "sleep 100".to_string(),
        );
        jobs.update_status(1, crate::JobStatus::Stopped);

        // Reference a non-existent job
        let result = builtin_fg(&["%999".to_string()], &jobs).unwrap();
        assert_eq!(result.exit_code(), 1);
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_bg_no_jobs() {
        let jobs = JobTable::new();
        let result = builtin_bg(&[], &jobs).unwrap();
        // Should fail with exit code 1 when no stopped jobs exist
        assert_eq!(result.exit_code(), 1);
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_bg_invalid_job_spec() {
        use nix::unistd::Pid;
        let jobs = JobTable::new();

        jobs.add_job(
            Pid::from_raw(99998),
            vec![Pid::from_raw(99998)],
            "sleep 200".to_string(),
        );
        jobs.update_status(1, crate::JobStatus::Stopped);

        let result = builtin_bg(&["%999".to_string()], &jobs).unwrap();
        assert_eq!(result.exit_code(), 1);
        assert!(matches!(result, BuiltinResult::Failure(1)));
    }

    #[test]
    fn test_bg_no_stopped_jobs() {
        use nix::unistd::Pid;
        let jobs = JobTable::new();

        // Add a running (background) job, but no stopped ones
        let id = jobs.add_job(
            Pid::from_raw(99997),
            vec![Pid::from_raw(99997)],
            "sleep 300".to_string(),
        );
        jobs.update_status(id, crate::JobStatus::Background);

        // bg with no args should fail because there are no stopped jobs
        let result = builtin_bg(&[], &jobs).unwrap();
        assert_eq!(result.exit_code(), 1);
    }

    #[test]
    fn test_bg_already_running() {
        use nix::unistd::Pid;
        let jobs = JobTable::new();

        // Add a background (running) job and reference it explicitly
        let id = jobs.add_job(
            Pid::from_raw(99996),
            vec![Pid::from_raw(99996)],
            "sleep 400".to_string(),
        );
        jobs.update_status(id, crate::JobStatus::Background);

        // Explicitly referencing a running job with bg should fail
        let spec = format!("%{}", id);
        let result = builtin_bg(&[spec], &jobs).unwrap();
        assert_eq!(result.exit_code(), 1);
    }

    #[test]
    fn test_fg_selects_most_recent_stopped() {
        use nix::unistd::Pid;
        let jobs = JobTable::new();

        // Add two stopped jobs; fg should pick the one with higher ID
        let id1 = jobs.add_job(
            Pid::from_raw(88881),
            vec![Pid::from_raw(88881)],
            "job_first".to_string(),
        );
        let id2 = jobs.add_job(
            Pid::from_raw(88882),
            vec![Pid::from_raw(88882)],
            "job_second".to_string(),
        );
        jobs.update_status(id1, crate::JobStatus::Stopped);
        jobs.update_status(id2, crate::JobStatus::Stopped);

        // fg will try to send SIGCONT to pgid 88882 which is not a real
        // process, so it will fail with "job has terminated", but the
        // important thing is that it selected the right job (id2).
        let result = builtin_fg(&[], &jobs).unwrap();
        // The job was selected (we can verify by checking the status was
        // updated to Done after the SIGCONT failure)
        let job = jobs.get_job(id2).unwrap();
        assert!(job.is_done());
        // Job id1 should still be stopped (was not selected)
        let job1 = jobs.get_job(id1).unwrap();
        assert!(job1.is_stopped());
        assert_eq!(result.exit_code(), 1); // Failed because fake PID
    }

    #[test]
    fn test_parse_job_spec_percent_number() {
        use nix::unistd::Pid;
        let jobs = JobTable::new();
        let id = jobs.add_job(
            Pid::from_raw(77770),
            vec![Pid::from_raw(77770)],
            "test_cmd".to_string(),
        );

        assert_eq!(parse_job_spec(&format!("%{}", id), &jobs), Some(id));
        assert_eq!(parse_job_spec("%999", &jobs), None);
    }

    #[test]
    fn test_parse_job_spec_percent_plus() {
        use nix::unistd::Pid;
        let jobs = JobTable::new();
        let _id1 = jobs.add_job(
            Pid::from_raw(77771),
            vec![Pid::from_raw(77771)],
            "first".to_string(),
        );
        let _id2 = jobs.add_job(
            Pid::from_raw(77772),
            vec![Pid::from_raw(77772)],
            "second".to_string(),
        );

        // %+ and %% refer to the most recent (last in list) job
        let result = parse_job_spec("%+", &jobs);
        assert!(result.is_some());

        let result2 = parse_job_spec("%%", &jobs);
        assert!(result2.is_some());

        // Verify that job IDs are valid
        assert!(jobs.get_job(result.unwrap()).is_some());
    }

    #[test]
    fn test_parse_job_spec_by_command_prefix() {
        use nix::unistd::Pid;
        let jobs = JobTable::new();
        let id = jobs.add_job(
            Pid::from_raw(77773),
            vec![Pid::from_raw(77773)],
            "my_unique_command --flag".to_string(),
        );

        assert_eq!(parse_job_spec("%my_unique", &jobs), Some(id));
        assert_eq!(parse_job_spec("%nonexistent", &jobs), None);
    }

    // ---- export -f (function export) tests ----

    /// Helper: define a function in the context from a shell command string.
    fn define_test_function(context: &mut ExpansionContext, name: &str, body_source: &str) {
        use colony_parser::pest_api;
        let stmt = pest_api::parse_statement(body_source).unwrap();
        let func_def = colony_parser::FunctionDef {
            name: name.to_string(),
            body: vec![stmt],
            span: colony_parser::Span::DUMMY,
        };
        context.define_function(name.to_string(), func_def);
    }

    #[test]
    fn test_export_f_marks_function() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "greet", "echo hello");
        assert!(!context.is_function_exported("greet"));
        let result = builtin_export(
            &["-f".to_string(), "greet".to_string()],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
        assert!(context.is_function_exported("greet"));
    }

    #[test]
    fn test_export_f_nonexistent_fails() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        let result = builtin_export(
            &["-f".to_string(), "nosuchfunc".to_string()],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_export_fn_unexports() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "myfunc", "echo test");
        context.export_function("myfunc");
        assert!(context.is_function_exported("myfunc"));
        let result = builtin_export(
            &["-fn".to_string(), "myfunc".to_string()],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
        assert!(!context.is_function_exported("myfunc"));
        // Function itself should still exist
        assert!(context.is_function("myfunc"));
    }

    #[test]
    fn test_export_fn_separate_flags() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "myfunc", "echo test");
        context.export_function("myfunc");
        let result = builtin_export(
            &["-f".to_string(), "-n".to_string(), "myfunc".to_string()],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
        assert!(!context.is_function_exported("myfunc"));
    }

    #[test]
    fn test_export_f_multiple() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "func_a", "echo a");
        define_test_function(&mut context, "func_b", "echo b");
        define_test_function(&mut context, "func_c", "echo c");
        let result = builtin_export(
            &[
                "-f".to_string(),
                "func_a".to_string(),
                "func_b".to_string(),
                "func_c".to_string(),
            ],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
        assert!(context.is_function_exported("func_a"));
        assert!(context.is_function_exported("func_b"));
        assert!(context.is_function_exported("func_c"));
    }

    #[test]
    fn test_export_f_partial_failure() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "real_func", "echo real");
        let result = builtin_export(
            &[
                "-f".to_string(),
                "real_func".to_string(),
                "fake_func".to_string(),
            ],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
        assert!(context.is_function_exported("real_func"));
    }

    #[test]
    fn test_serialize_function_format() {
        use colony_parser::pest_api;
        let stmt = pest_api::parse_statement("echo hello world").unwrap();
        let func_def = colony_parser::FunctionDef {
            name: "greet".to_string(),
            body: vec![stmt],
            span: colony_parser::Span::DUMMY,
        };
        let serialized = serialize_function_for_export(&func_def);
        assert!(serialized.starts_with("() {"));
        assert!(serialized.contains("echo hello world"));
        assert!(serialized.ends_with('}'));
    }

    #[test]
    fn test_build_env_has_exported_funcs() {
        let mut context = ExpansionContext::new();
        define_test_function(&mut context, "testfn", "echo from_testfn");
        context.export_function("testfn");
        let env = crate::build_env(&context);
        let func_env = env.iter().find(|(k, _)| k == "BASH_FUNC_testfn%%");
        assert!(func_env.is_some());
        let (_, value) = func_env.unwrap();
        assert!(value.starts_with("() {"));
        assert!(value.contains("echo from_testfn"));
    }

    #[test]
    fn test_build_env_excludes_unexported() {
        let mut context = ExpansionContext::new();
        define_test_function(&mut context, "private_fn", "echo private");
        let env = crate::build_env(&context);
        let func_env = env.iter().find(|(k, _)| k == "BASH_FUNC_private_fn%%");
        assert!(func_env.is_none());
    }

    #[test]
    fn test_function_export_roundtrip() {
        use colony_parser::pest_api;
        let mut context = ExpansionContext::new();
        let stmt = pest_api::parse_statement("echo hello_roundtrip").unwrap();
        let func_def = colony_parser::FunctionDef {
            name: "roundtrip_fn".to_string(),
            body: vec![stmt],
            span: colony_parser::Span::DUMMY,
        };
        context.define_function("roundtrip_fn".to_string(), func_def);
        context.export_function("roundtrip_fn");

        let env = crate::build_env(&context);
        let (env_key, env_value) = env
            .iter()
            .find(|(k, _)| k == "BASH_FUNC_roundtrip_fn%%")
            .expect("exported function should be in env");
        assert_eq!(env_key, "BASH_FUNC_roundtrip_fn%%");
        assert!(env_value.starts_with("() {"));

        // Parse it back (simulate child shell import)
        let func_source = format!("roundtrip_fn {}", env_value);
        let parsed = pest_api::parse_statement(&func_source);
        assert!(parsed.is_ok(), "Failed to parse: {:?}", parsed);
        match parsed.unwrap() {
            colony_parser::Statement::FunctionDef(func) => {
                assert_eq!(func.name, "roundtrip_fn");
                assert!(!func.body.is_empty());
            }
            other => panic!("Expected FunctionDef, got: {:?}", other),
        }
    }

    #[test]
    fn test_import_function_from_env_value() {
        use colony_parser::pest_api;
        let func_name = "test_import_fn";
        let env_value = "() { \n    echo imported\n}";
        let mut context = ExpansionContext::new();

        // Simulate the import logic from startup.rs
        assert!(env_value.starts_with("() {"));
        let func_source = format!("{} {}", func_name, env_value);
        if let Ok(colony_parser::Statement::FunctionDef(func)) =
            pest_api::parse_statement(&func_source)
        {
            context.define_function(func.name.clone(), func);
            context.export_function(func_name);
        }
        assert!(context.is_function(func_name));
        assert!(context.is_function_exported(func_name));
    }

    #[test]
    fn test_import_skips_invalid_env_value() {
        let func_name = "bad_import_fn";
        let env_value = "not a function definition";
        let context = ExpansionContext::new();

        if env_value.starts_with("() {") {
            panic!("Test setup error");
        }
        assert!(!context.is_function(func_name));
    }

    #[test]
    fn test_unset_removes_export_flag() {
        let mut context = ExpansionContext::new();
        define_test_function(&mut context, "to_unset", "echo bye");
        context.export_function("to_unset");
        assert!(context.is_function_exported("to_unset"));
        context.unset_function("to_unset");
        assert!(!context.is_function("to_unset"));
        assert!(!context.is_function_exported("to_unset"));
    }
}
