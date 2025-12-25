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
use colony_expand::{ExpansionContext, Expander, Variable, CompletionSpec, CompletionAction};
use colony_parser::{SimpleCommand, Word};
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
}

impl BuiltinResult {
    pub fn exit_code(&self) -> i32 {
        match self {
            BuiltinResult::Success(code) 
            | BuiltinResult::Failure(code) 
            | BuiltinResult::Exit(code) => *code,
            BuiltinResult::Source(_, _) => 0,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, BuiltinResult::Success(0))
    }

    pub fn is_exit(&self) -> bool {
        matches!(self, BuiltinResult::Exit(_))
    }
    
    pub fn is_source(&self) -> bool {
        matches!(self, BuiltinResult::Source(_, _))
    }
}

pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "cd" | "export" | "unset" | "exit" | "pwd" | "echo" | "type"
        | "alias" | "unalias" | "." | "source"
        | "fg" | "bg" | "jobs" | "wait"
        | "[" | "test"
        | ":"
        | "context"
        | "declare" | "local"
        | "true" | "false"
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
        | "dirs" | "pushd" | "popd"
        | "complete" | "compgen"
        | "mapfile" | "readarray"
    )
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
pub fn execute_builtin(
    cmd: &SimpleCommand,
    context: &mut ExpansionContext,
    jobs: &JobTable,
    tuple_space: &TupleSpace,
    channels: &mut ChannelManager,
) -> Option<Result<BuiltinResult>> {
    // Expand command name (with command substitution support)
    let mut expander = Expander::with_executor(context.clone(), |stmt, ctx| {
        crate::execute_and_capture(stmt, ctx)
    });
    let name_fields = match expander.expand_word(&cmd.name) {
        Ok(fields) => fields,
        Err(e) => return Some(Err(e.into())),
    };

    if name_fields.is_empty() {
        return None;
    }

    let name = &name_fields[0].text;

    if !is_builtin(name) {
        return None;
    }

    // Expand arguments - include extra fields from name expansion (e.g., "$@" -> multiple fields)
    let mut args: Vec<String> = name_fields[1..].iter().map(|f| f.text.clone()).collect();
    for (i, arg_word) in cmd.args.iter().enumerate() {
        let fields = match expander.expand_word(arg_word) {
            Ok(fields) => {
                fields
            }
            Err(e) => return Some(Err(e.into())),
        };
        for field in fields {
            args.push(field.text);
        }
    }

    // Dispatch to specific builtin
    let result = match name.as_str() {
        "cd" => builtin_cd(&args, context, tuple_space),
        "export" => builtin_export(&args, context, tuple_space),
        "unset" => builtin_unset(&args, context, tuple_space),
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
        "pushd" => builtin_pushd(&args, context, tuple_space),
        "popd" => builtin_popd(&args, context, tuple_space),
        "complete" => builtin_complete(&args, context),
        "compgen" => builtin_compgen(&args, context),
        "mapfile" | "readarray" => builtin_mapfile(&args, context),
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
/// **Reactive:** Emits ("dir_change", old_dir, new_dir) tuple (Rule 46)
fn builtin_cd(args: &[String], context: &mut ExpansionContext, tuple_space: &TupleSpace) -> Result<BuiltinResult> {
    let old_dir = env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "/".to_string());

    let target = if args.is_empty() {
        context
            .get_env("HOME")
            .ok_or_else(|| ExecutionError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "HOME not set",
            )))?
    } else {
        &args[0]
    };

    env::set_current_dir(target).map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(
            e.kind(),
            format!("cd: {}: {}", target, e),
        ))
    })?;

    let new_dir = env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| target.to_string());

    context.set_env("OLDPWD", &old_dir);
    context.set_env("PWD", &new_dir);

    tuple_space.out(Tuple::from(vec![
        Value::Str("dir_change".to_string()),
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
fn builtin_export(args: &[String], context: &mut ExpansionContext, tuple_space: &TupleSpace) -> Result<BuiltinResult> {
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
            for name in context.exported_function_names() {
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
        return Ok(if exit_code == 0 { BuiltinResult::Success(0) } else { BuiltinResult::Failure(1) });
    }
    
    if names.is_empty() {
        for (key, value) in env::vars() {
            println!("export {}={}", key, value);
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
                Value::Str(name.to_string()),
                Value::Str(old_value),
                Value::Str(value.to_string()),
            ]));
        } else {
            if let Some(value) = context.get_env(arg) {
                env::set_var(arg, value);
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// unset - Remove environment variables
///
/// Usage: unset VAR [VAR2 ...]
///
/// **Reactive:** Emits ("env_change", var, old, null) tuple (Rule 43)
fn builtin_unset(args: &[String], context: &mut ExpansionContext, tuple_space: &TupleSpace) -> Result<BuiltinResult> {
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
        
        let old_value = context.get_var_or_env(var_name).unwrap_or_default();

        context.unset_variable(var_name);
        context.unset_env(var_name);
        env::remove_var(var_name);

        tuple_space.out(Tuple::from(vec![
            Value::Str("env_change".to_string()),
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
    let cwd = env::current_dir().map_err(|e| {
        ExecutionError::IoError(std::io::Error::new(e.kind(), format!("pwd: {}", e)))
    })?;

    println!("{}", cwd.display());
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
        if opt_chars.is_empty() || !opt_chars.iter().all(|c| *c == 'n' || *c == 'e' || *c == 'E') {
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
    if output.1 {
        let _ = std::io::stdout().write_all(output.0.as_bytes());
    } else if no_newline {
        let _ = std::io::stdout().write_all(output.0.as_bytes());
    } else {
        let _ = writeln!(std::io::stdout(), "{}", output.0);
    }
    let _ = std::io::stdout().flush();

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
                            if next >= '0' && next <= '7' {
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
        "nounset" => opts.nounset = enable,
        "xtrace" => opts.xtrace = enable,
        "noglob" => opts.noglob = enable,
        "noexec" => opts.noexec = enable,
        "verbose" => opts.verbose = enable,
        _ => {}
    }
}

/// Shell reserved keywords
const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi",
    "case", "esac", "for", "while", "until", "do", "done",
    "in", "function", "select", "time", "coproc",
    "!", "{", "}", "[[", "]]",
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
            if !suppress_functions && !path_only {
                if context.get_function(name).is_some() {
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
            } else if let Ok(path) = crate::path::resolve_command(name, env::var("PATH").ok().as_deref()) {
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
/// - alias              - List all aliases
/// - alias name         - Show specific alias
/// - alias name='value' - Define alias
///
/// POSIX: Aliases substitute the command name before execution
fn builtin_alias(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        // No args: list all aliases
        let mut aliases: Vec<_> = context.aliases().iter().collect();
        aliases.sort_by_key(|(name, _)| *name);

        for (name, value) in aliases {
            println!("alias {}='{}'", name, value);
        }
        return Ok(BuiltinResult::Success(0));
    }

    // Process each argument
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            // Define alias: name=value
            let (name, value) = arg.split_at(eq_pos);
            let value = &value[1..]; // Skip the '='

            // Remove surrounding quotes if present
            let value = value.trim_matches('\'').trim_matches('"');

            context.set_alias(name.to_string(), value.to_string());
        } else {
            // Show specific alias
            if let Some(value) = context.get_alias(arg) {
                println!("alias {}='{}'", arg, value);
            } else {
                eprintln!("alias: {}: not found", arg);
                return Ok(BuiltinResult::Failure(1));
            }
        }
    }

    Ok(BuiltinResult::Success(0))
}

/// unalias - Remove command aliases
///
/// Usage: unalias name [name...]
///
/// Removes the specified aliases from the alias table.
fn builtin_unalias(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    if args.is_empty() {
        eprintln!("unalias: usage: unalias name [name ...]");
        return Ok(BuiltinResult::Failure(1));
    }

    for name in args {
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

/// fg - Bring job to foreground
///
/// Usage: fg [%job_id]
/// TODO: Implement terminal control and SIGCONT
fn builtin_fg(_args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    let job_list = jobs.list_jobs();

    if job_list.is_empty() {
        eprintln!("fg: no current job");
        return Ok(BuiltinResult::Failure(1));
    }

    // For now, just list what would be foregrounded
    eprintln!("fg: would foreground job (not yet implemented)");
    Ok(BuiltinResult::Failure(1))
}

/// bg - Continue job in background
///
/// Usage: bg [%job_id]
/// TODO: Implement SIGCONT to stopped jobs
fn builtin_bg(_args: &[String], jobs: &JobTable) -> Result<BuiltinResult> {
    let job_list = jobs.list_jobs();

    if job_list.is_empty() {
        eprintln!("bg: no current job");
        return Ok(BuiltinResult::Failure(1));
    }

    eprintln!("bg: would background job (not yet implemented)");
    Ok(BuiltinResult::Failure(1))
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
    
    // No arguments: wait for all children
    if args.is_empty() {
        // Try to reap any zombie children
        loop {
            match waitpid(Pid::from_raw(-1), Some(nix::sys::wait::WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::StillAlive) | Err(_) => break,
                Ok(_) => continue, // Reaped one, try for more
            }
        }
        return Ok(BuiltinResult::Success(0));
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

    let result = evaluate_test_expr(args)?;
    Ok(if result {
        BuiltinResult::Success(0)
    } else {
        BuiltinResult::Failure(1)
    })
}

fn evaluate_test_expr(args: &[String]) -> Result<bool> {
    if args.is_empty() {
        return Ok(false);
    }
    
    let mut idx = 0;
    evaluate_test_or(&args, &mut idx)
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
                if matches!(next_next.as_str(), "=" | "==" | "!=" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | "<" | ">") {
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
                "-v" => {
                    false
                }
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
        
        if matches!(op.as_str(), "=" | "==" | "!=" | "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" | "<" | ">" | "-nt" | "-ot" | "-ef") {
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
            eprintln!("test: unknown operator: {}", op);
            return Ok(false);
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

/// context - Manage context channel (fd 3)
///
/// Usage:
/// - context add <text>     - Append text to context channel
/// - context show           - Display all context entries
/// - context show --last N  - Show last N entries
/// - context clear          - Clear context channel
///
/// **F024:** Extended channels system - writes to stdcontext (fd 3)
fn builtin_context(args: &[String], channels: &mut ChannelManager) -> Result<BuiltinResult> {
    use colony_channels::Channel;

    if args.is_empty() {
        eprintln!("context: missing subcommand");
        eprintln!("Usage: context <add|show|clear> [args...]");
        return Ok(BuiltinResult::Failure(1));
    }

    let subcommand = &args[0];

    match subcommand.as_str() {
        "add" => {
            if args.len() < 2 {
                eprintln!("context add: missing text");
                return Ok(BuiltinResult::Failure(1));
            }

            // Join all remaining args as the text to add
            let text = args[1..].join(" ");

            // Get mutable reference to context channel
            if let Some(ctx_channel) = channels.context() {
                // Use ContextChannel's add_text method
                ctx_channel.add_text(&text);
                println!("Added to context: {}", text);
                Ok(BuiltinResult::Success(0))
            } else {
                eprintln!("context: channel not available");
                Ok(BuiltinResult::Failure(1))
            }
        }

        "show" => {
            // Check for --last N option
            let count_limit = if args.len() >= 3 && args[1] == "--last" {
                args[2].parse::<usize>().ok()
            } else {
                None
            };

            // Get context channel
            if let Some(ctx_channel) = channels.context() {
                let entries = if let Some(limit) = count_limit {
                    ctx_channel.last(limit)
                } else {
                    ctx_channel.lines()
                };

                if entries.is_empty() {
                    println!("(context is empty)");
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
                // Call clear through Channel trait
                ctx_channel.clear();
                println!("Context cleared");
                Ok(BuiltinResult::Success(0))
            } else {
                eprintln!("context: channel not available");
                Ok(BuiltinResult::Failure(1))
            }
        }

        _ => {
            eprintln!("context: unknown subcommand: {}", subcommand);
            eprintln!("Usage: context <add|show|clear> [args...]");
            Ok(BuiltinResult::Failure(1))
        }
    }
}

fn builtin_declare(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use std::collections::HashMap;
    use colony_expand::VarAttributes;
    
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
                    'F' => { function_mode = true; function_names_only = true; }
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
                    let attr_flags = context.get_var_attributes(name)
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
                    let export_flag = if context.is_function_exported(name) { "x" } else { "" };
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
                        let export_flag = if context.is_function_exported(name) { "x" } else { "" };
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
                    let inner = &value_part[1..value_part.len()-1];
                    let mut map = HashMap::new();
                    for item in inner.split_whitespace() {
                        if item.starts_with('[') {
                            if let Some(end) = item.find("]=") {
                                let key = &item[1..end];
                                let val = &item[end+2..];
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
                    let inner = &value_part[1..value_part.len()-1];
                    let elements: Vec<String> = inner.split_whitespace().map(String::from).collect();
                    context.set_variable(name.to_string(), Variable::Array(elements));
                } else {
                    context.set_variable(name.to_string(), Variable::Array(vec![value_part.to_string()]));
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
    if !context.in_function() {
        eprintln!("local: can only be used in a function");
        return Ok(BuiltinResult::Failure(1));
    }

    if args.is_empty() {
        return Ok(BuiltinResult::Success(0));
    }

    let mut is_nameref = false;
    let mut var_args: Vec<&str> = Vec::new();
    
    for arg in args {
        if arg == "-n" {
            is_nameref = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            for c in arg[1..].chars() {
                if c == 'n' {
                    is_nameref = true;
                }
            }
        } else {
            var_args.push(arg);
        }
    }

    for arg in var_args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            if is_nameref {
                context.declare_local(name, Some(Variable::NameRef(value.to_string())));
            } else {
                context.declare_local(name, Some(Variable::String(value.to_string())));
            }
        } else if is_nameref {
            context.declare_local(arg, Some(Variable::NameRef(String::new())));
        } else {
            context.declare_local(arg, None);
        }
    }

    Ok(BuiltinResult::Success(0))
}

fn builtin_read(args: &[String], context: &mut ExpansionContext) -> Result<BuiltinResult> {
    use std::io::{self, BufRead, Read};
    
    let mut raw_mode = false;
    let mut var_names: Vec<&str> = Vec::new();
    
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-r" {
            raw_mode = true;
        } else if arg.starts_with('-') {
            i += 1;
            continue;
        } else {
            var_names.push(arg);
        }
        i += 1;
    }
    
    if var_names.is_empty() {
        var_names.push("REPLY");
    }
    
    let stdin = io::stdin();
    let mut stdin_lock = stdin.lock();
    let mut line = String::new();
    
    if raw_mode {
        match stdin_lock.read_line(&mut line) {
            Ok(0) => {
                for name in &var_names {
                    context.set_var(name.to_string(), String::new());
                }
                return Ok(BuiltinResult::Success(1));
            }
            Ok(_) => {}
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
    
    if line.ends_with('\n') {
        line.pop();
    }
    if line.ends_with('\r') {
        line.pop();
    }
    
    let ifs = context.ifs();
    let ifs_chars: Vec<char> = ifs.chars().collect();
    let ifs_whitespace: Vec<char> = ifs_chars.iter()
        .filter(|c| c.is_whitespace())
        .copied()
        .collect();
    
    if var_names.len() == 1 && var_names[0] == "REPLY" {
        context.set_var("REPLY".to_string(), line);
        return Ok(BuiltinResult::Success(0));
    }
    
    let trimmed = line.trim_start_matches(|c: char| ifs_whitespace.contains(&c));
    
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut j = 0;
    
    while j < chars.len() {
        let c = chars[j];
        if ifs_chars.contains(&c) {
            if !current.is_empty() || !ifs_whitespace.contains(&c) {
                fields.push(current);
                current = String::new();
            }
            while j + 1 < chars.len() && ifs_whitespace.contains(&chars[j + 1]) {
                j += 1;
            }
        } else {
            current.push(c);
        }
        j += 1;
    }
    if !current.is_empty() {
        fields.push(current);
    }
    
    for (idx, name) in var_names.iter().enumerate() {
        if idx < fields.len() {
            if idx == var_names.len() - 1 && idx + 1 < fields.len() {
                let rest: Vec<&str> = fields[idx..].iter().map(|s| s.as_str()).collect();
                context.set_var(name.to_string(), rest.join(" "));
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
    use std::io::{self, BufRead, Read};
    
    // Parse options
    let mut strip_newlines = false;
    let mut count_limit: usize = 0;  // 0 means no limit
    let mut origin: usize = 0;
    let mut skip_count: usize = 0;
    let mut delimiter: u8 = b'\n';  // default newline
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
                        'n' | 'O' | 's' | 'd' => {
                            // These options need arguments
                            // Check if the rest of the string is the argument
                            if j + 1 < chars.len() {
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
                            } else {
                                eprintln!("mapfile: {}: invalid option", s);
                                return Ok(BuiltinResult::Failure(1));
                            }
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
        print!("{}", output);
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
    use colony_parser::{Statement, ListOp, Condition};
    
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
            let cmds: Vec<String> = pipeline.elements.iter()
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
                result.push_str(&format!("\nelif {}; then\n", format_condition(&elif.condition)));
                result.push_str(&format_statements_indented(&elif.body));
            }
            if let Some(else_body) = &if_stmt.else_body {
                result.push_str("\nelse\n");
                result.push_str(&format_statements_indented(else_body));
            }
            result.push_str("\nfi");
            result
        }
        Statement::While(while_stmt) => {
            format!("while {}; do\n{}\ndone", 
                format_condition(&while_stmt.condition),
                format_statements_indented(&while_stmt.body))
        }
        Statement::Until(until_stmt) => {
            format!("until {}; do\n{}\ndone",
                format_condition(&until_stmt.condition),
                format_statements_indented(&until_stmt.body))
        }
        Statement::For(for_stmt) => {
            if for_stmt.words.is_empty() {
                format!("for {}; do\n{}\ndone",
                    for_stmt.var,
                    format_statements_indented(&for_stmt.body))
            } else {
                let words: Vec<String> = for_stmt.words.iter().map(format_word).collect();
                format!("for {} in {}; do\n{}\ndone",
                    for_stmt.var,
                    words.join(" "),
                    format_statements_indented(&for_stmt.body))
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
            format!("{} () {{\n{}\n}}", func.name, format_function_body(&func.body))
        }
        Statement::BraceGroup(group) => {
            format!("{{\n{}\n}}", format_statements_indented(&group.body))
        }
        Statement::Subshell(sub) => {
            format!("(\n{}\n)", format_statements_indented(&sub.body))
        }
        Statement::VarDecl(decl) => {
            format!("{}={}", decl.name, format_expr(&decl.value))
        }
        Statement::Return(ret) => {
            if let Some(ref val) = ret.value {
                format!("return {}", format_expr(val))
            } else {
                "return".to_string()
            }
        }
        _ => "# unsupported".to_string()
    }
}

fn format_condition(cond: &colony_parser::Condition) -> String {
    match cond {
        colony_parser::Condition::Command(stmt) => format_statement(stmt),
        colony_parser::Condition::Expression(expr) => format_expr(expr),
    }
}

fn format_statements_indented(stmts: &[colony_parser::Statement]) -> String {
    stmts.iter()
        .map(|s| format!("    {}", format_statement(s)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_statements_double_indented(stmts: &[colony_parser::Statement]) -> String {
    stmts.iter()
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
        Expression::Binary { left, op, right, .. } => {
            format!("{} {:?} {}", format_expr(left), op, format_expr(right))
        }
        _ => String::new()
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
                        while i < chars.len() && chars[i] >= '0' && chars[i] <= '7' && octal.len() < 3 {
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
                    format_number(num, 10, false, width, precision, left_justify, zero_pad, plus_sign, space_sign)
                }
                'u' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    format_unsigned(num, 10, false, width, left_justify, zero_pad)
                }
                'o' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0" } else { "" };
                    format_unsigned_with_prefix(num, 8, false, width, left_justify, zero_pad, prefix)
                }
                'x' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0x" } else { "" };
                    format_unsigned_with_prefix(num, 16, false, width, left_justify, zero_pad, prefix)
                }
                'X' => {
                    let num: u64 = arg.parse().unwrap_or(0);
                    let prefix = if alternate && num != 0 { "0X" } else { "" };
                    format_unsigned_with_prefix(num, 16, true, width, left_justify, zero_pad, prefix)
                }
                'c' => {
                    // First character
                    arg.chars().next().map(|c| c.to_string()).unwrap_or_default()
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
                format!("{}{:0>width$}", sign, num_str, width = num_str.len() + padding)
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
            format!("{}{:0>width$}", prefix, num_str, width = num_str.len() + padding)
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
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/') {
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
    use colony_expand::{TrapSignal, TrapAction};
    
    if args.is_empty() {
        let traps = context.traps();
        if traps.is_empty() {
            return Ok(BuiltinResult::Success(0));
        }
        for (signal, action) in traps {
            match action {
                TrapAction::Ignore => println!("trap -- '' {}", signal),
                TrapAction::Command(cmd) => println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal),
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
                    TrapAction::Command(cmd) => println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal),
                    TrapAction::Default => {}
                }
            }
        } else {
            for sig_name in remaining_args {
                if let Some(signal) = TrapSignal::parse(sig_name) {
                    if let Some(action) = context.get_trap(&signal) {
                        match action {
                            TrapAction::Ignore => println!("trap -- '' {}", signal),
                            TrapAction::Command(cmd) => println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal),
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
                TrapAction::Command(cmd) => println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal),
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
                    TrapAction::Command(cmd) => println!("trap -- '{}' {}", cmd.replace('\'', "'\\''"), signal),
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
    if !context.in_function() {
        eprintln!("return: can only be used in a function");
        return Ok(BuiltinResult::Failure(1));
    }
    
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
    let (silent_mode, effective_optstring) = if optstring.starts_with(':') {
        (true, &optstring[1..])
    } else {
        (false, optstring.as_str())
    };
    
    // Get OPTERR (0 = suppress errors, default = 1)
    let opterr = context
        .get_var_or_env("OPTERR")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(1) != 0;
    
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
            if saved_optind == optind || (optind == saved_optind) {
                return (saved_optind, char_pos);
            }
        }
    }
    
    (optind, 1)
}

/// Save getopts internal state to _GETOPTS_STATE variable
fn save_getopts_state(context: &mut ExpansionContext, arg_index: usize, char_pos: usize) {
    context.set_var("_GETOPTS_STATE".to_string(), format!("{}:{}", arg_index, char_pos));
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
    if arg.starts_with('+') {
        arg[1..].parse::<usize>().ok().filter(|&n| n < stack_len)
    } else if arg.starts_with('-') && arg.len() > 1 && arg.chars().nth(1).unwrap().is_ascii_digit() {
        let n = arg[1..].parse::<usize>().ok()?;
        if n < stack_len {
            Some(stack_len - 1 - n)
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
            s if s.starts_with('+') || (s.starts_with('-') && s.len() > 1 && s.chars().nth(1).unwrap().is_ascii_digit()) => {
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
        let cwd = stack.first().cloned().unwrap_or_else(|| env::current_dir().unwrap_or_default());
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

fn builtin_pushd(args: &[String], context: &mut ExpansionContext, tuple_space: &TupleSpace) -> Result<BuiltinResult> {
    let mut no_cd = false;
    let mut target: Option<&str> = None;
    let mut index_arg: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "-n" => no_cd = true,
            s if s.starts_with('+') || (s.starts_with('-') && s.len() > 1 && s.chars().nth(1).unwrap().is_ascii_digit()) => {
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
                let rotated: Vec<PathBuf> = stack[idx..].iter().chain(stack[..idx].iter()).cloned().collect();
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
                Value::Str(old_dir.display().to_string()),
                Value::Str(new_dir.display().to_string()),
            ]));
        }
        
        print_dir_stack(context, home.as_deref());
        return Ok(BuiltinResult::Success(0));
    }

    let target_path = PathBuf::from(target.unwrap());
    
    if !target_path.is_dir() {
        eprintln!("pushd: {}: No such file or directory", target_path.display());
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
            Value::Str(old_dir.display().to_string()),
            Value::Str(target_canonical.display().to_string()),
        ]));
    }

    print_dir_stack(context, home.as_deref());
    Ok(BuiltinResult::Success(0))
}

fn builtin_popd(args: &[String], context: &mut ExpansionContext, tuple_space: &TupleSpace) -> Result<BuiltinResult> {
    let mut no_cd = false;
    let mut index_arg: Option<&str> = None;

    for arg in args {
        match arg.as_str() {
            "-n" => no_cd = true,
            s if s.starts_with('+') || (s.starts_with('-') && s.len() > 1 && s.chars().nth(1).unwrap().is_ascii_digit()) => {
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
        let new_dir = context.dir_stack().first().cloned().unwrap_or_else(|| env::current_dir().unwrap_or_default());
        
        if let Err(e) = env::set_current_dir(&new_dir) {
            context.dir_stack_mut().insert(0, old_dir);
            eprintln!("popd: {}: {}", new_dir.display(), e);
            return Ok(BuiltinResult::Failure(1));
        }
        
        context.set_env("OLDPWD", old_dir.to_str().unwrap_or(""));
        context.set_env("PWD", new_dir.to_str().unwrap_or(""));
        
        tuple_space.out(Tuple::from(vec![
            Value::Str("dir_change".to_string()),
            Value::Str(old_dir.display().to_string()),
            Value::Str(new_dir.display().to_string()),
        ]));
    }

    print_dir_stack(context, home.as_deref());
    Ok(BuiltinResult::Success(0))
}

fn print_dir_stack(context: &ExpansionContext, home: Option<&std::path::Path>) {
    let stack = context.dir_stack();
    let entries: Vec<String> = stack
        .iter()
        .map(|p| tilde_abbreviate(p, home))
        .collect();
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
                    if (set_mode && value) || (unset_mode && !value) {
                        if !quiet_mode {
                            if print_mode {
                                println!("shopt {} {}", if value { "-s" } else { "-u" }, name);
                            } else {
                                println!("{}\t{}", name, if value { "on" } else { "off" });
                            }
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

fn generate_completions(spec: &CompletionSpec, context: &ExpansionContext, word_prefix: Option<&str>) -> Vec<String> {
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
                for (name, _) in context.aliases() {
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
                for (name, _) in context.functions() {
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
                let pattern = if prefix.is_empty() { "*" } else { &format!("{}*", prefix) };
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
                let pattern = if prefix.is_empty() { "*" } else { &format!("{}*", prefix) };
                if let Ok(entries) = glob::glob(pattern) {
                    for entry in entries.flatten() {
                        completions.push(entry.display().to_string());
                    }
                }
            }
            CompletionAction::Function => {
                for (name, _) in context.functions() {
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
                for kw in &["if", "then", "else", "elif", "fi", "case", "esac", "for", "while", 
                            "until", "do", "done", "in", "function", "select", "time", "coproc",
                            "{", "}", "!", "[[", "]]"] {
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
                for sig in &["HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "BUS", "FPE", 
                            "KILL", "USR1", "SEGV", "USR2", "PIPE", "ALRM", "TERM",
                            "STKFLT", "CHLD", "CONT", "STOP", "TSTP", "TTIN", "TTOU",
                            "URG", "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "IO", "PWR", "SYS"] {
                    if sig.starts_with(prefix) || format!("SIG{}", sig).starts_with(prefix) {
                        completions.push(sig.to_string());
                    }
                }
            }
            CompletionAction::SetOpt => {
                for opt in &["errexit", "nounset", "xtrace", "noglob", "noexec", "verbose"] {
                    if opt.starts_with(prefix) {
                        completions.push(opt.to_string());
                    }
                }
            }
            CompletionAction::ShOpt => {
                for opt in &["extglob", "globstar", "nullglob", "dotglob", "failglob", 
                            "nocaseglob", "nocasematch", "expand_aliases", "histappend"] {
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

fn get_builtin_names() -> Vec<&'static str> {
    vec![
        ".", ":", "[", "alias", "bg", "break", "builtin", "cd", "command", 
        "compgen", "complete", "compopt", "continue", "declare", "dirs",
        "disown", "echo", "enable", "eval", "exec", "exit", "export", 
        "false", "fc", "fg", "getopts", "hash", "help", "history", "jobs",
        "kill", "let", "local", "logout", "mapfile", "popd", "printf",
        "pushd", "pwd", "read", "readarray", "readonly", "return", "set",
        "shift", "shopt", "source", "suspend", "test", "times", "trap",
        "true", "type", "typeset", "ulimit", "umask", "unalias", "unset",
        "wait",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_parser::Span;

    fn make_simple_command(name: &str, args: Vec<&str>) -> SimpleCommand {
        SimpleCommand {
            name: Word::literal(name, Span::DUMMY),
            args: args
                .into_iter()
                .map(|s| Word::literal(s, Span::DUMMY))
                .collect(),
            redirects: vec![],
            env_assignments: vec![],
            span: Span::DUMMY,
        }
    }

    #[test]
    fn test_is_builtin() {
        assert!(is_builtin("cd"));
        assert!(is_builtin("export"));
        assert!(is_builtin("echo"));
        assert!(is_builtin("pwd"));
        assert!(is_builtin("exit"));
        assert!(!is_builtin("ls"));
        assert!(!is_builtin("grep"));
    }

    #[test]
    fn test_builtin_pwd() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("pwd", vec![]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_none()); // ls is not a builtin
    }

    #[test]
    fn test_builtin_jobs_empty() {
        let mut context = ExpansionContext::new();
        let jobs = JobTable::new();
        let tuple_space = TupleSpace::new();
        let mut channels = ChannelManager::new();
        let cmd = make_simple_command("jobs", vec![]);

        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
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
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        // Check tuple was emitted
        let pattern = Tuple::pattern(&[
            Value::Str("env_change".to_string()),
            Value::Str("REACTIVE_TEST".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]);

        let tuple = tuple_space.inp(pattern);
        assert!(tuple.is_some());

        let t = tuple.unwrap();
        assert_eq!(t.fields[0], Value::Str("env_change".to_string()));
        assert_eq!(t.fields[1], Value::Str("REACTIVE_TEST".to_string()));
        assert_eq!(t.fields[2], Value::Str("".to_string())); // old value (was empty)
        assert_eq!(t.fields[3], Value::Str("initial".to_string())); // new value
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
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        // Check tuple was emitted
        let pattern = Tuple::pattern(&[
            Value::Str("dir_change".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]);

        let tuple = tuple_space.inp(pattern);
        assert!(tuple.is_some());

        let t = tuple.unwrap();
        assert_eq!(t.fields[0], Value::Str("dir_change".to_string()));
        // fields[1] is old dir, fields[2] is new dir (should be /tmp or /private/tmp)
        // Just verify it's a string value
        assert!(matches!(t.fields[2], Value::Str(_)));

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
        let result = execute_builtin(&cmd, &mut context, &jobs, &tuple_space, &mut channels);
        assert!(result.is_some());

        let pattern = Tuple::pattern(&[
            Value::Str("env_change".to_string()),
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
        
        let result = builtin_set(&["--".to_string(), "a".to_string(), "b".to_string(), "c".to_string()], &mut context);
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
        
        let result = builtin_set(&["-ex".to_string(), "--".to_string(), "a".to_string(), "b".to_string()], &mut context);
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
}
