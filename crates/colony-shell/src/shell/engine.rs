use colony_exec::Executor;
use colony_parser::{AliasContext, Statement};
use miette::Result;
use nix::unistd::Pid; // only if still used by coproc
use crate::shell::execution::{
    execute_exit_trap,
    execute_statement_for_code,
    execute_statement_with_control,
    statement_to_shell_string,
    ControlFlow,
};

// ========================= Engine Overview =========================
// Purpose: Thin façade around shell::execution modules plus legacy helpers
//          (conditional/arithmetic evaluation, coproc plumbing). Control flow,
//          traps, backgrounding, and serialization now live under execution/.
// Entrypoints:
//   - execute_command_with_status: `-c` path; parses a full program string,
//     executes via execution::execute_statement_with_control, updates last_exit.
//   - execute_shell_command: REPL/orchestrator single-statement path; parses one
//     statement and routes through execution::execute_statement_for_code.
// Execution flow:
//   parsing -> execution::execute_statement_with_control -> traps/background
//   side-effects (last_exit_status, last_bg_pid) are applied in Executor context.
// =====================================================================

/// Parse and execute a program string (-c path), updating last exit status.
pub(crate) async fn execute_command_with_status(line: &str, executor: &mut Executor) -> i32 {
    use colony_parser::pest_api::parse_with_aliases;

    let mut alias_ctx = AliasContext::from_map(executor.context().aliases().clone());
    let program = match parse_with_aliases(line, &mut alias_ctx) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("parse error in -c: {}\n  input: {}", e, line);
            return 2;
        }
    };

    let mut last_exit = 0;
    for stmt in &program.statements {
        let (code, flow) = execute_statement_with_control(stmt, executor).await;
        last_exit = code;
        executor.context_mut().set_last_exit_status(code);
        if flow == ControlFlow::Exit {
            if let Some(trap_code) = execute_exit_trap(executor, execute_statement_with_control).await {
                return trap_code;
            }
            return code;
        }
    }
    
    if let Some(trap_code) = execute_exit_trap(executor, execute_statement_with_control).await {
        return trap_code;
    }
    last_exit
}

/// Parse and execute a single statement for REPL/orchestrator, updating last exit status.
pub(crate) async fn execute_shell_command(line: &str, executor: &mut Executor) -> Result<()> {
    let mut alias_ctx = AliasContext::from_map(executor.context().aliases().clone());
    let stmt = colony_parser::pest_api::parse_statement_with_aliases(line, &mut alias_ctx)
        .map_err(|e| miette::miette!("parse error: {}", e))?;

    let exit_code = execute_statement_for_code(&stmt, executor).await;
    executor.context_mut().set_last_exit_status(exit_code);
    
    if exit_code != 0 {
        eprintln!("Command exited with code {}", exit_code);
    }

    Ok(())
}

/// Launch a coprocess: set up pipes, fork, exec the coproc command, and register handles.
pub(crate) async fn execute_coproc(coproc: &colony_parser::CoprocCommand, executor: &mut Executor) -> Result<i32, String> {
    use nix::sys::signal::{signal, SigHandler, Signal};
    use nix::unistd::{fork, ForkResult, pipe, dup2, close, execvp, setpgid};
    use std::ffi::CString;
    use std::os::fd::AsRawFd;

    let coproc_name = coproc.name.clone().unwrap_or_else(|| "COPROC".to_string());

    let stdin_pipe = pipe().map_err(|e| format!("pipe error: {}", e))?;
    let stdout_pipe = pipe().map_err(|e| format!("pipe error: {}", e))?;

    let is_simple_external = matches!(coproc.command.as_ref(), Statement::SimpleCommand(cmd)
        if cmd.name.as_literal().is_some() && !colony_exec::is_builtin(cmd.name.as_literal().unwrap()));

    let shell_cmd = if !is_simple_external {
        Some(statement_to_shell_string(coproc.command.as_ref()))
    } else {
        None
    };

    let shell_exec = if let Some(cmd) = shell_cmd {
        let exe_path = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "colony".to_string());
        let c_exe = CString::new(exe_path).map_err(|e| format!("exec path error: {}", e))?;
        let c_flag = CString::new("-c").map_err(|e| format!("flag error: {}", e))?;
        let c_cmd = CString::new(cmd).map_err(|e| format!("command error: {}", e))?;
        Some((c_exe, c_flag, c_cmd))
    } else {
        None
    };

    let external_exec = if is_simple_external {
        if let Statement::SimpleCommand(cmd) = coproc.command.as_ref() {
            let cmd_name = cmd.name.as_literal().unwrap_or("true");
            let c_cmd = CString::new(cmd_name).map_err(|e| format!("arg error: {}", e))?;
            let mut c_args: Vec<CString> = vec![c_cmd.clone()];
            for arg in &cmd.args {
                if let Some(lit) = arg.as_literal() {
                    c_args.push(CString::new(lit).map_err(|e| format!("arg error: {}", e))?);
                }
            }
            Some((c_cmd, c_args))
        } else {
            None
        }
    } else {
        None
    };

    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            unsafe {
                let _ = signal(Signal::SIGINT, SigHandler::SigDfl);
                let _ = signal(Signal::SIGQUIT, SigHandler::SigDfl);
                let _ = signal(Signal::SIGTSTP, SigHandler::SigDfl);
            }

            close(stdin_pipe.1.as_raw_fd()).ok();
            close(stdout_pipe.0.as_raw_fd()).ok();
            dup2(stdin_pipe.0.as_raw_fd(), 0).ok();
            dup2(stdout_pipe.1.as_raw_fd(), 1).ok();
            close(stdin_pipe.0.as_raw_fd()).ok();
            close(stdout_pipe.1.as_raw_fd()).ok();

            if let Err(err) = setpgid(Pid::from_raw(0), Pid::from_raw(0)) {
                eprintln!("coproc setpgid failed: {err}");
                unsafe { nix::libc::_exit(1); }
            }

            if let Some((c_exe, c_flag, c_cmd)) = shell_exec {
                let args: Vec<&std::ffi::CStr> = vec![c_exe.as_c_str(), c_flag.as_c_str(), c_cmd.as_c_str()];
                let _ = execvp(&c_exe, &args);
                eprintln!("coproc exec failed");
                unsafe { nix::libc::_exit(127); }
            } else if let Some((c_cmd, c_args)) = external_exec {
                let c_args_ref: Vec<&std::ffi::CStr> = c_args.iter().map(|s| s.as_c_str()).collect();
                let _ = execvp(&c_cmd, &c_args_ref);
                eprintln!("coproc exec failed");
                unsafe { nix::libc::_exit(127); }
            } else {
                unsafe { nix::libc::_exit(1); }
            }
        }
        Ok(ForkResult::Parent { child }) => {
            drop(stdin_pipe.0);
            drop(stdout_pipe.1);

            let read_fd = stdout_pipe.0.as_raw_fd();
            let write_fd = stdin_pipe.1.as_raw_fd();

            executor.context_mut().set_array_element(&coproc_name, 0, &read_fd.to_string());
            executor.context_mut().set_array_element(&coproc_name, 1, &write_fd.to_string());
            executor.context_mut().set_var(format!("{}_PID", coproc_name), child.as_raw().to_string());

            std::mem::forget(stdout_pipe.0);
            std::mem::forget(stdin_pipe.1);

            Ok(0)
        }
        Err(e) => Err(format!("fork error: {}", e)),
    }
}

/// Evaluate a POSIX arithmetic command and return its numeric result.
pub(crate) fn evaluate_arithmetic_command(expr: &str, executor: &mut Executor) -> i64 {
    match colony_expand::expand_arithmetic(expr, executor.context_mut()) {
        Ok(result) => result.parse().unwrap_or(0),
        Err(_) => 0,
    }
}

/// Evaluate a C-style arithmetic expression (init/update forms) mutating context as needed.
pub(crate) fn evaluate_cstyle_arith_expr(expr: &str, executor: &mut Executor) -> i64 {
    let expr = expr.trim();
    if expr.is_empty() {
        return 0;
    }
    
    let exprs: Vec<&str> = expr.split(',').collect();
    if exprs.len() > 1 {
        let mut result = 0;
        for e in exprs {
            result = evaluate_cstyle_arith_single(e.trim(), executor);
        }
        return result;
    }
    
    evaluate_cstyle_arith_single(expr, executor)
}

/// Evaluate a C-style arithmetic value expression without mutating context.
pub(crate) fn evaluate_cstyle_arith_value(expr: &str, executor: &Executor) -> i64 {
    let expanded = expand_arithmetic_vars(expr, executor);
    evaluate_arithmetic_expr(&expanded)
}

/// Evaluate a [ … ] conditional expression against the current executor context.
pub(crate) fn evaluate_conditional(expr: &colony_parser::CondExpr, executor: &mut Executor) -> bool {
    use colony_parser::{CondExpr, CondBinaryOp, CondUnaryOp};
    
    match expr {
        CondExpr::And(left, right) => {
            evaluate_conditional(left, executor) && evaluate_conditional(right, executor)
        }
        CondExpr::Or(left, right) => {
            evaluate_conditional(left, executor) || evaluate_conditional(right, executor)
        }
        CondExpr::Not(inner) => {
            !evaluate_conditional(inner, executor)
        }
        CondExpr::Binary { left, op, right, right_quoted } => {
            let left_expanded = expand_cond_operand(left, executor);
            let right_expanded = expand_cond_operand(right, executor);
            
            match op {
                CondBinaryOp::Eq => {
                    if *right_quoted {
                        left_expanded == right_expanded
                    } else {
                        glob_match(&right_expanded, &left_expanded)
                    }
                }
                CondBinaryOp::Ne => {
                    if *right_quoted {
                        left_expanded != right_expanded
                    } else {
                        !glob_match(&right_expanded, &left_expanded)
                    }
                }
                CondBinaryOp::Match => {
                    match regex::Regex::new(&right_expanded) {
                        Ok(re) => {
                            if let Some(caps) = re.captures(&left_expanded) {
                                let mut bash_rematch = Vec::new();
                                for i in 0..caps.len() {
                                    bash_rematch.push(caps.get(i).map(|m| m.as_str()).unwrap_or("").to_string());
                                }
                                executor.context_mut().set_variable(
                                    "BASH_REMATCH".to_string(),
                                    colony_expand::Variable::Array(bash_rematch),
                                );
                                true
                            } else {
                                executor.context_mut().set_variable(
                                    "BASH_REMATCH".to_string(),
                                    colony_expand::Variable::Array(vec![]),
                                );
                                false
                            }
                        }
                        Err(_) => false,
                    }
                }
                CondBinaryOp::Lt => left_expanded < right_expanded,
                CondBinaryOp::Gt => left_expanded > right_expanded,
                CondBinaryOp::IntEq => {
                    left_expanded.parse::<i64>().ok() == right_expanded.parse::<i64>().ok()
                }
                CondBinaryOp::IntNe => {
                    left_expanded.parse::<i64>().ok() != right_expanded.parse::<i64>().ok()
                }
                CondBinaryOp::IntLt => {
                    match (left_expanded.parse::<i64>(), right_expanded.parse::<i64>()) {
                        (Ok(l), Ok(r)) => l < r,
                        _ => false,
                    }
                }
                CondBinaryOp::IntLe => {
                    match (left_expanded.parse::<i64>(), right_expanded.parse::<i64>()) {
                        (Ok(l), Ok(r)) => l <= r,
                        _ => false,
                    }
                }
                CondBinaryOp::IntGt => {
                    match (left_expanded.parse::<i64>(), right_expanded.parse::<i64>()) {
                        (Ok(l), Ok(r)) => l > r,
                        _ => false,
                    }
                }
                CondBinaryOp::IntGe => {
                    match (left_expanded.parse::<i64>(), right_expanded.parse::<i64>()) {
                        (Ok(l), Ok(r)) => l >= r,
                        _ => false,
                    }
                }
                CondBinaryOp::NewerThan => {
                    use std::fs;
                    match (fs::metadata(&left_expanded), fs::metadata(&right_expanded)) {
                        (Ok(m1), Ok(m2)) => m1.modified().ok() > m2.modified().ok(),
                        _ => false,
                    }
                }
                CondBinaryOp::OlderThan => {
                    use std::fs;
                    match (fs::metadata(&left_expanded), fs::metadata(&right_expanded)) {
                        (Ok(m1), Ok(m2)) => m1.modified().ok() < m2.modified().ok(),
                        _ => false,
                    }
                }
                CondBinaryOp::SameFile => {
                    use std::os::unix::fs::MetadataExt;
                    use std::fs;
                    match (fs::metadata(&left_expanded), fs::metadata(&right_expanded)) {
                        (Ok(m1), Ok(m2)) => m1.dev() == m2.dev() && m1.ino() == m2.ino(),
                        _ => false,
                    }
                }
            }
        }
        CondExpr::Unary { op, arg } => {
            let arg_expanded = expand_cond_operand(arg, executor);
            let path = std::path::Path::new(&arg_expanded);
            
            match op {
                CondUnaryOp::NonZeroLen => !arg_expanded.is_empty(),
                CondUnaryOp::ZeroLen => arg_expanded.is_empty(),
                CondUnaryOp::IsFile => path.is_file(),
                CondUnaryOp::IsDir => path.is_dir(),
                CondUnaryOp::Exists => path.exists(),
                CondUnaryOp::Readable => {
                    use nix::unistd::access;
                    use nix::unistd::AccessFlags;
                    access(path, AccessFlags::R_OK).is_ok()
                }
                CondUnaryOp::Writable => {
                    use nix::unistd::access;
                    use nix::unistd::AccessFlags;
                    access(path, AccessFlags::W_OK).is_ok()
                }
                CondUnaryOp::Executable => {
                    use std::os::unix::fs::PermissionsExt;
                    path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
                }
                CondUnaryOp::NonEmpty => {
                    path.metadata().map(|m| m.len() > 0).unwrap_or(false)
                }
                CondUnaryOp::IsSymlink => path.is_symlink(),
                CondUnaryOp::IsPipe => {
                    use std::os::unix::fs::FileTypeExt;
                    path.metadata().map(|m| m.file_type().is_fifo()).unwrap_or(false)
                }
                CondUnaryOp::IsSocket => {
                    use std::os::unix::fs::FileTypeExt;
                    path.metadata().map(|m| m.file_type().is_socket()).unwrap_or(false)
                }
                CondUnaryOp::IsBlock => {
                    use std::os::unix::fs::FileTypeExt;
                    path.metadata().map(|m| m.file_type().is_block_device()).unwrap_or(false)
                }
                CondUnaryOp::IsChar => {
                    use std::os::unix::fs::FileTypeExt;
                    path.metadata().map(|m| m.file_type().is_char_device()).unwrap_or(false)
                }
                CondUnaryOp::IsTerminal => {
                    arg_expanded.parse::<i32>().map(|fd| unsafe { libc::isatty(fd) } != 0).unwrap_or(false)
                }
                CondUnaryOp::OwnedByEuid => {
                    use std::os::unix::fs::MetadataExt;
                    path.metadata().map(|m| m.uid() == unsafe { libc::geteuid() }).unwrap_or(false)
                }
                CondUnaryOp::OwnedByEgid => {
                    use std::os::unix::fs::MetadataExt;
                    path.metadata().map(|m| m.gid() == unsafe { libc::getegid() }).unwrap_or(false)
                }
                CondUnaryOp::ModifiedSinceRead => {
                    use std::os::unix::fs::MetadataExt;
                    path.metadata().map(|m| m.mtime() > m.atime()).unwrap_or(false)
                }
                CondUnaryOp::VarIsSet => {
                    executor.context().get_var_or_env(&arg_expanded).is_some()
                }
            }
        }
        CondExpr::String(s) => {
            let expanded = expand_cond_operand(s, executor);
            !expanded.is_empty()
        }
    }
}

/// Recursive evaluator for arithmetic subexpressions (operators ||, &&, comparisons, +, -, *, /, %).
fn evaluate_arithmetic_expr(expr: &str) -> i64 {
    let expr = expr.trim();
    
    if let Some(idx) = expr.find("||") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left != 0 || right != 0 { 1 } else { 0 };
    }
    if let Some(idx) = expr.find("&&") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left != 0 && right != 0 { 1 } else { 0 };
    }
    
    if let Some(idx) = expr.find(">=") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left >= right { 1 } else { 0 };
    }
    if let Some(idx) = expr.find("<=") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left <= right { 1 } else { 0 };
    }
    if let Some(idx) = expr.find("==") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left == right { 1 } else { 0 };
    }
    if let Some(idx) = expr.find("!=") {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+2..]);
        return if left != right { 1 } else { 0 };
    }
    if let Some(idx) = expr.rfind('>') {
        if idx > 0 && !expr[..idx].ends_with('>') && !expr[..idx].ends_with('|') && !expr[..idx].ends_with('=') {
            let left = evaluate_arithmetic_expr(&expr[..idx]);
            let right = evaluate_arithmetic_expr(&expr[idx+1..]);
            return if left > right { 1 } else { 0 };
        }
    }
    if let Some(idx) = expr.rfind('<') {
        if idx > 0 && !expr[..idx].ends_with('<') && !expr[..idx].ends_with('|') && !expr[..idx].ends_with('=') {
            let left = evaluate_arithmetic_expr(&expr[..idx]);
            let right = evaluate_arithmetic_expr(&expr[idx+1..]);
            return if left < right { 1 } else { 0 };
        }
    }
    
    if let Some(idx) = expr.rfind('+') {
        if idx > 0 {
            let left = evaluate_arithmetic_expr(&expr[..idx]);
            let right = evaluate_arithmetic_expr(&expr[idx+1..]);
            return left + right;
        }
    }
    if let Some(idx) = expr.rfind('-') {
        if idx > 0 {
            let left = evaluate_arithmetic_expr(&expr[..idx]);
            let right = evaluate_arithmetic_expr(&expr[idx+1..]);
            return left - right;
        }
    }
    if let Some(idx) = expr.rfind('*') {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+1..]);
        return left * right;
    }
    if let Some(idx) = expr.rfind('/') {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+1..]);
        if right != 0 {
            return left / right;
        }
        return 0;
    }
    if let Some(idx) = expr.rfind('%') {
        let left = evaluate_arithmetic_expr(&expr[..idx]);
        let right = evaluate_arithmetic_expr(&expr[idx+1..]);
        if right != 0 {
            return left % right;
        }
        return 0;
    }
    
    if expr.starts_with('(') && expr.ends_with(')') {
        return evaluate_arithmetic_expr(&expr[1..expr.len()-1]);
    }
    
    if let Some(stripped) = expr.strip_prefix('-') {
        return -evaluate_arithmetic_expr(stripped);
    }
    
    expr.parse().unwrap_or(0)
}

/// Evaluate a single C-style arithmetic term (++, --, compound assignments) mutating context.
fn evaluate_cstyle_arith_single(expr: &str, executor: &mut Executor) -> i64 {
    let expr = expr.trim();
    if expr.is_empty() {
        return 0;
    }
    
    if expr.starts_with("++") {
        let var = &expr[2..].trim();
        let current = get_var_as_i64(var, executor);
        let new_val = current + 1;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    if expr.starts_with("--") {
        let var = &expr[2..].trim();
        let current = get_var_as_i64(var, executor);
        let new_val = current - 1;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    
    if expr.ends_with("++") {
        let var = expr[..expr.len()-2].trim();
        let current = get_var_as_i64(var, executor);
        executor.context_mut().set_var(var.to_string(), (current + 1).to_string());
        return current;
    }
    if expr.ends_with("--") {
        let var = expr[..expr.len()-2].trim();
        let current = get_var_as_i64(var, executor);
        executor.context_mut().set_var(var.to_string(), (current - 1).to_string());
        return current;
    }
    
    if let Some(idx) = expr.find("+=") {
        let var = expr[..idx].trim();
        let val_expr = expr[idx+2..].trim();
        let current = get_var_as_i64(var, executor);
        let delta = evaluate_cstyle_arith_value(val_expr, executor);
        let new_val = current + delta;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    if let Some(idx) = expr.find("-=") {
        let var = expr[..idx].trim();
        let val_expr = expr[idx+2..].trim();
        let current = get_var_as_i64(var, executor);
        let delta = evaluate_cstyle_arith_value(val_expr, executor);
        let new_val = current - delta;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    if let Some(idx) = expr.find("*=") {
        let var = expr[..idx].trim();
        let val_expr = expr[idx+2..].trim();
        let current = get_var_as_i64(var, executor);
        let factor = evaluate_cstyle_arith_value(val_expr, executor);
        let new_val = current * factor;
        executor.context_mut().set_var(var.to_string(), new_val.to_string());
        return new_val;
    }
    if let Some(idx) = expr.find("/=") {
        let var = expr[..idx].trim();
        let val_expr = expr[idx+2..].trim();
        let current = get_var_as_i64(var, executor);
        let divisor = evaluate_cstyle_arith_value(val_expr, executor);
        if divisor != 0 {
            let new_val = current / divisor;
            executor.context_mut().set_var(var.to_string(), new_val.to_string());
            return new_val;
        }
        return 0;
    }
    
    if let Some(idx) = expr.find('=') {
        if idx > 0 && !matches!(expr.chars().nth(idx.saturating_sub(1)), Some('!') | Some('<') | Some('>') | Some('=')) 
            && !matches!(expr.chars().nth(idx+1), Some('=')) {
            let var = expr[..idx].trim();
            let val_expr = expr[idx+1..].trim();
            let new_val = evaluate_cstyle_arith_value(val_expr, executor);
            executor.context_mut().set_var(var.to_string(), new_val.to_string());
            return new_val;
        }
    }
    
    evaluate_cstyle_arith_value(expr, executor)
}

/// Expand variables/command substitutions inside an arithmetic string.
fn expand_arithmetic_vars(expr: &str, executor: &Executor) -> String {
    let mut result = String::new();
    let chars_vec: Vec<char> = expr.chars().collect();
    let mut i = 0;
    
    while i < chars_vec.len() {
        let c = chars_vec[i];
        if c == '$' && i + 1 < chars_vec.len() && chars_vec[i+1] == '(' {
            i += 2;
            let start = i;
            let mut paren_depth = 1;
            while i < chars_vec.len() && paren_depth > 0 {
                if chars_vec[i] == '(' { paren_depth += 1; }
                else if chars_vec[i] == ')' { paren_depth -= 1; }
                if paren_depth > 0 { i += 1; }
            }
            let cmd_str: String = chars_vec[start..i].iter().collect();
            if i < chars_vec.len() { i += 1; }
            
            if let Ok(program) = colony_parser::pest_api::parse(&cmd_str) {
                let mut ctx = executor.context().clone();
                let mut output = String::new();
                for stmt in &program.statements {
                    if let Ok(out) = colony_exec::execute_and_capture(stmt, &mut ctx) {
                        output.push_str(&out);
                    }
                }
                result.push_str(output.trim_end_matches('\n'));
            }
        } else if c == '$' {
            i += 1;
            let mut var_name = String::new();
            if i < chars_vec.len() && chars_vec[i] == '{' {
                i += 1;
                while i < chars_vec.len() && chars_vec[i] != '}' {
                    var_name.push(chars_vec[i]);
                    i += 1;
                }
                if i < chars_vec.len() { i += 1; }
            } else {
                while i < chars_vec.len() && (chars_vec[i].is_alphanumeric() || chars_vec[i] == '_') {
                    var_name.push(chars_vec[i]);
                    i += 1;
                }
            }
            let val = executor.context().get_var_or_env(&var_name).unwrap_or_default();
            result.push_str(&val);
        } else if c.is_alphabetic() || c == '_' {
            let mut var_name = String::from(c);
            i += 1;
            while i < chars_vec.len() && (chars_vec[i].is_alphanumeric() || chars_vec[i] == '_') {
                var_name.push(chars_vec[i]);
                i += 1;
            }
            if i < chars_vec.len() && chars_vec[i] == '[' {
                i += 1;
                let mut index = String::new();
                let mut bracket_depth = 1;
                while i < chars_vec.len() && bracket_depth > 0 {
                    if chars_vec[i] == '[' {
                        bracket_depth += 1;
                    } else if chars_vec[i] == ']' {
                        bracket_depth -= 1;
                        if bracket_depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    index.push(chars_vec[i]);
                    i += 1;
                }
                let expanded_index = expand_arithmetic_vars(&index, executor);
                let idx: usize = evaluate_arithmetic_expr(&expanded_index) as usize;
                
                if let Some(colony_expand::Variable::Array(arr)) = executor.context().get_variable(&var_name) {
                    if let Some(val) = arr.get(idx) {
                        result.push_str(val);
                    } else {
                        result.push('0');
                    }
                } else {
                    result.push('0');
                }
            } else {
                if let Some(val) = executor.context().get_var_or_env(&var_name) {
                    result.push_str(&val);
                } else {
                    result.push_str(&var_name);
                }
            }
        } else {
            result.push(c);
            i += 1;
        }
    }
    
    result
}

/// Expand a conditional operand (handles quotes, vars).
fn expand_cond_operand(s: &str, executor: &Executor) -> String {
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        s[1..s.len()-1].to_string()
    } else if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        expand_double_quoted(&s[1..s.len()-1], executor)
    } else if s.starts_with('$') {
        expand_variable_ref(s, executor)
    } else {
        s.to_string()
    }
}

/// Expand vars/command subs inside a double-quoted string.
fn expand_double_quoted(content: &str, executor: &Executor) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '$' {
            let mut var_str = String::from("$");
            if chars.peek() == Some(&'{') {
                var_str.push(chars.next().unwrap());
                while let Some(&ch) = chars.peek() {
                    var_str.push(chars.next().unwrap());
                    if ch == '}' {
                        break;
                    }
                }
            } else if chars.peek() == Some(&'(') {
                var_str.push(chars.next().unwrap());
                let mut depth = 1;
                while depth > 0 {
                    if let Some(ch) = chars.next() {
                        var_str.push(ch);
                        if ch == '(' { depth += 1; }
                        if ch == ')' { depth -= 1; }
                    } else {
                        break;
                    }
                }
            } else {
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '[' || ch == ']' {
                        var_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
            }
            result.push_str(&expand_variable_ref(&var_str, executor));
        } else if c == '\\' {
            if let Some(escaped) = chars.next() {
                match escaped {
                    '$' | '"' | '\\' | '`' => result.push(escaped),
                    _ => {
                        result.push('\\');
                        result.push(escaped);
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    
    result
}

/// Expand a variable reference, handling ${} and array subscripts.
fn expand_variable_ref(s: &str, executor: &Executor) -> String {
    use colony_expand::Variable;
    
    if !s.starts_with('$') {
        return s.to_string();
    }
    
    let content = if s.starts_with("${") && s.ends_with('}') {
        &s[2..s.len()-1]
    } else {
        &s[1..]
    };
    
    if let Some(bracket_pos) = content.find('[') {
        if let Some(end_pos) = content.find(']') {
            let var_name = &content[..bracket_pos];
            let index_str = &content[bracket_pos+1..end_pos];
            
            if let Some(Variable::Array(arr)) = executor.context().get_variable(var_name) {
                if let Ok(index) = index_str.parse::<usize>() {
                    return arr.get(index).cloned().unwrap_or_default();
                } else if index_str == "@" || index_str == "*" {
                    return arr.join(" ");
                }
            }
            return String::new();
        }
    }
    
    executor.context().get_var_or_env(content).unwrap_or_default()
}

/// Perform shell-style glob match of pattern against text.
fn glob_match(pattern: &str, text: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();
    
    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                if pattern_chars.peek().is_none() {
                    return true;
                }
                let remaining_pattern: String = pattern_chars.collect();
                let text_remaining: Vec<char> = text_chars.collect();
                for i in 0..=text_remaining.len() {
                    if glob_match(&remaining_pattern, &text_remaining[i..].iter().collect::<String>()) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                if text_chars.next().is_none() {
                    return false;
                }
            }
            '[' => {
                let text_char = match text_chars.next() {
                    Some(c) => c,
                    None => return false,
                };
                
                let negated = pattern_chars.peek() == Some(&'!') || pattern_chars.peek() == Some(&'^');
                if negated {
                    pattern_chars.next();
                }
                
                let mut matches = false;
                let mut prev_char: Option<char> = None;
                let mut first_char = true;
                
                loop {
                    match pattern_chars.next() {
                        Some(']') if !first_char => break,
                        Some('[') if pattern_chars.peek() == Some(&':') => {
                            first_char = false;
                            pattern_chars.next();
                            let mut class_name = String::new();
                            while let Some(&c) = pattern_chars.peek() {
                                if c == ':' {
                                    pattern_chars.next();
                                    if pattern_chars.peek() == Some(&']') {
                                        pattern_chars.next();
                                        break;
                                    }
                                    class_name.push(':');
                                } else {
                                    class_name.push(pattern_chars.next().unwrap());
                                }
                            }
                            matches = matches || match class_name.as_str() {
                                "alpha" => text_char.is_alphabetic(),
                                "digit" => text_char.is_ascii_digit(),
                                "alnum" => text_char.is_alphanumeric(),
                                "space" => text_char.is_whitespace(),
                                "lower" => text_char.is_lowercase(),
                                "upper" => text_char.is_uppercase(),
                                "punct" => text_char.is_ascii_punctuation(),
                                "xdigit" => text_char.is_ascii_hexdigit(),
                                "blank" => text_char == ' ' || text_char == '\t',
                                "cntrl" => text_char.is_ascii_control(),
                                "graph" => text_char.is_ascii_graphic(),
                                "print" => !text_char.is_ascii_control() || text_char == ' ',
                                _ => false,
                            };
                            prev_char = None;
                        }
                        Some('-') if prev_char.is_some() && pattern_chars.peek() != Some(&']') => {
                            if let Some(end_char) = pattern_chars.next() {
                                if let (Some(start), end) = (prev_char, end_char) {
                                    matches = matches || (text_char >= start && text_char <= end);
                                }
                            }
                            prev_char = None;
                        }
                        Some(c) => {
                            first_char = false;
                            if let Some(pc) = prev_char {
                                matches = matches || text_char == pc;
                            }
                            prev_char = Some(c);
                        }
                        None => return false,
                    }
                }
                
                if let Some(pc) = prev_char {
                    matches = matches || text_char == pc;
                }
                
                if negated {
                    matches = !matches;
                }
                
                if !matches {
                    return false;
                }
            }
            '\\' => {
                if let Some(next) = pattern_chars.next() {
                    if text_chars.next() != Some(next) {
                        return false;
                    }
                }
            }
            c => {
                if text_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }
    
    text_chars.next().is_none()
}

/// Fetch variable as i64 (defaults to 0 on missing/parsing errors).
fn get_var_as_i64(var: &str, executor: &Executor) -> i64 {
    executor.context().get_var_or_env(var)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
