use colony_exec::{Executor, ExecutionError};
use colony_expand::{TrapAction, TrapSignal};
use colony_parser::{AliasContext, Statement};
use miette::Result;
use nix::unistd::Pid;
use tracing::{info, warn};



/// Execute a list with control flow propagation

const MAX_JOBS: usize = 128;

async fn execute_list_with_control(list: &colony_parser::CommandList, executor: &mut Executor) -> (i32, ControlFlow) {
    use colony_parser::ListOp;
    use colony_exec::JobStatus;
    use nix::unistd::{fork, ForkResult, Pid};
    use nix::sys::wait::{waitpid, WaitPidFlag};

    // Check if first command should be backgrounded
    // This happens when rest starts with Background operator
    let first_is_background = list.rest.first().map(|(op, _)| *op == ListOp::Background).unwrap_or(false);
    
    // Check if first command is followed by && or || (errexit exemption)
    let first_is_errexit_exempt = list.rest.first().map(|(op, _)| matches!(op, ListOp::And | ListOp::Or)).unwrap_or(false);
    
    let (mut last_exit_code, mut control) = if first_is_background {
        // Run first command in background
        match run_in_background(&*list.first, executor).await {
            Ok((job_id, pid)) => {
                if executor.context().is_interactive() {
                    eprintln!("[{}] {}", job_id, pid);
                }
                (0, ControlFlow::Normal)
            }
            Err(_) => (1, ControlFlow::Normal)
        }
    } else {
        execute_statement_with_errexit_control(&*list.first, executor, first_is_errexit_exempt).await
    };
    executor.context_mut().set_last_exit_status(last_exit_code);

    // If we got break/continue, propagate it immediately
    if control != ControlFlow::Normal {
        return (last_exit_code, control);
    }

    // Execute rest of list based on operators
    let mut i = 0;
    while i < list.rest.len() {
        let (op, stmt) = &list.rest[i];
        
        // Check if NEXT operator is Background (meaning this stmt should be backgrounded)
        let next_is_background = list.rest.get(i + 1).map(|(next_op, _)| *next_op == ListOp::Background).unwrap_or(false);
        // Check if NEXT operator is && or || (errexit exemption)
        let next_is_errexit_exempt = list.rest.get(i + 1).map(|(next_op, _)| matches!(next_op, ListOp::And | ListOp::Or)).unwrap_or(false);
        // Also check if this is the last item and the op itself is Background (trailing &)
        let this_is_background = *op == ListOp::Background;
        
        let should_execute = match op {
            ListOp::Semi => true,
            ListOp::And => last_exit_code == 0,
            ListOp::Or => last_exit_code != 0,
            ListOp::Background => {
                // After a background, continue with next command
                true
            }
        };

        if should_execute {
            if next_is_background || (this_is_background && i == list.rest.len() - 1) {
                // Skip if this_is_background - the previous already handled it
                if !this_is_background {
                    match run_in_background(stmt, executor).await {
                        Ok((job_id, pid)) => {
                            if executor.context().is_interactive() {
                                eprintln!("[{}] {}", job_id, pid);
                            }
                            last_exit_code = 0;
                        }
                        Err(err) => {
                            eprintln!("background start failed: {}", err);
                            last_exit_code = 1;
                        }
                    }
                }
            } else if !this_is_background {
                // Run in foreground
                let (code, flow) = execute_statement_with_errexit_control(stmt, executor, next_is_errexit_exempt).await;
                last_exit_code = code;
                executor.context_mut().set_last_exit_status(last_exit_code);
                control = flow;

                // Propagate break/continue
                if control != ControlFlow::Normal {
                    return (last_exit_code, control);
                }
            }
        }
        
        i += 1;
    }

    (last_exit_code, control)
}

async fn run_in_background(stmt: &colony_parser::Statement, executor: &mut Executor) -> std::result::Result<(usize, u32), String> {
    use colony_exec::JobStatus;
    use nix::sys::signal::{signal, SigHandler, Signal};
    use nix::unistd::{fork, setpgid, ForkResult, Pid};

    if executor.jobs().list_jobs().len() >= MAX_JOBS {
        return Err("too many background jobs".to_string());
    }

    let cmd_string = statement_to_shell_string(stmt);
    let mut child_context = executor.context().clone();

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            let pid = child.as_raw() as u32;
            executor.context().set_last_bg_pid(pid);

            let job_id = executor.jobs().add_job(child, vec![child], cmd_string.clone());
            executor.jobs().update_status(job_id, JobStatus::Background);
            info!(job_id, pid, cmd = %cmd_string, "background job started");

            Ok((job_id, pid))
        }
        Ok(ForkResult::Child) => {
            unsafe {
                let _ = signal(Signal::SIGINT, SigHandler::SigDfl);
                let _ = signal(Signal::SIGQUIT, SigHandler::SigDfl);
                let _ = signal(Signal::SIGTSTP, SigHandler::SigDfl);
            }

            if let Err(err) = setpgid(Pid::from_raw(0), Pid::from_raw(0)) {
                eprintln!("setpgid failed: {err}");
                unsafe { nix::libc::_exit(1); }
            }

            let exit_code = colony_exec::execute_statement_for_process_sub(stmt, &mut child_context);
            unsafe { nix::libc::_exit(exit_code); }
        }
        Err(err) => {
            Err(format!("fork failed: {err}"))
        }
    }
}

/// Execute if with control flow propagation
async fn execute_if_with_control(if_stmt: &colony_parser::IfStatement, executor: &mut Executor) -> (i32, ControlFlow) {
    use colony_parser::Condition;

    let saved_errexit = executor.context().shell_options().errexit;
    executor.context_mut().shell_options_mut().errexit = false;
    let (condition_exit, _) = match &if_stmt.condition {
        Condition::Command(cmd) => execute_statement_with_errexit_control(cmd, executor, true).await,
        Condition::Expression(_expr) => {
            eprintln!("Expression conditions not yet supported");
            (1, ControlFlow::Normal)
        }
    };
    executor.context_mut().shell_options_mut().errexit = saved_errexit;

    let condition_result = condition_exit == 0;

    if condition_result {
        let mut last_exit = 0;
        for stmt in &if_stmt.then_body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            if flow != ControlFlow::Normal {
                return (last_exit, flow);
            }
        }
        (last_exit, ControlFlow::Normal)
    } else {
        for elif in &if_stmt.elif_branches {
            let saved_errexit = executor.context().shell_options().errexit;
            executor.context_mut().shell_options_mut().errexit = false;
            let (elif_exit, _) = match &elif.condition {
                Condition::Command(cmd) => execute_statement_with_errexit_control(cmd, executor, true).await,
                Condition::Expression(_) => (1, ControlFlow::Normal),
            };
            executor.context_mut().shell_options_mut().errexit = saved_errexit;

            if elif_exit == 0 {
                let mut last_exit = 0;
                for stmt in &elif.body {
                    let (code, flow) = execute_statement_with_control(stmt, executor).await;
                    last_exit = code;
                    if flow != ControlFlow::Normal {
                        return (last_exit, flow);
                    }
                }
                return (last_exit, ControlFlow::Normal);
            }
        }

        // Execute else body if present
        if let Some(else_body) = &if_stmt.else_body {
            let mut last_exit = 0;
            for stmt in else_body {
                let (code, flow) = execute_statement_with_control(stmt, executor).await;
                last_exit = code;
                if flow != ControlFlow::Normal {
                    return (last_exit, flow);
                }
            }
            (last_exit, ControlFlow::Normal)
        } else {
            (0, ControlFlow::Normal)
        }
    }
}

/// Execute for loop with control flow returns
async fn execute_for_loop_with_control(for_stmt: &colony_parser::ForStatement, executor: &mut Executor) -> (i32, ControlFlow) {
    use colony_expand::Expander;

    // Expand the word list (with command substitution support)
    let mut expander = Expander::with_executor(executor.context().clone(), |stmt, ctx| {
        colony_exec::execute_and_capture(stmt, ctx)
    });
    let mut values = Vec::new();

    for word in &for_stmt.words {
        match expander.expand_word(word) {
            Ok(fields) => {
                values.extend(fields);
            }
            Err(e) => {
                eprintln!("expansion error: {}", e);
                return (1, ControlFlow::Normal);
            }
        }
    }

    // Iterate over values
    let mut last_exit = 0;
    for field in values {
        // Set loop variable
        executor.context_mut().set_var(for_stmt.var.clone(), field.text);

        // Execute body
        for stmt in &for_stmt.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;

            match flow {
                ControlFlow::Break => return (last_exit, ControlFlow::Normal),
                ControlFlow::Continue => break,
                ControlFlow::Exit => return (last_exit, ControlFlow::Exit),
                ControlFlow::Return => return (last_exit, ControlFlow::Return),
                ControlFlow::Normal => {}
            }
        }
    }

    (last_exit, ControlFlow::Normal)
}

/// Execute an if statement (old version for compatibility)
async fn execute_if_statement(if_stmt: &colony_parser::IfStatement, executor: &mut Executor) -> i32 {
    use colony_parser::Condition;

    // Evaluate the condition
    let condition_result = match &if_stmt.condition {
        Condition::Command(cmd) => {
            // Execute command and check exit code
            execute_statement_for_code(cmd, executor).await == 0
        }
        Condition::Expression(_expr) => {
            // Expression conditions not yet implemented
            eprintln!("Expression conditions not yet supported");
            false
        }
    };

    // Execute appropriate branch
    if condition_result {
        // Execute then body
        let mut last_exit = 0;
        for stmt in &if_stmt.then_body {
            last_exit = execute_statement_for_code(stmt, executor).await;
        }
        last_exit
    } else {
        // Check elif branches
        for elif in &if_stmt.elif_branches {
            let elif_result = match &elif.condition {
                Condition::Command(cmd) => {
                    execute_statement_for_code(cmd, executor).await == 0
                }
                Condition::Expression(_) => false,
            };

            if elif_result {
                let mut last_exit = 0;
                for stmt in &elif.body {
                    last_exit = execute_statement_for_code(stmt, executor).await;
                }
                return last_exit;
            }
        }

        // Execute else body if present
        if let Some(else_body) = &if_stmt.else_body {
            let mut last_exit = 0;
            for stmt in else_body {
                last_exit = execute_statement_for_code(stmt, executor).await;
            }
            last_exit
        } else {
            0  // No else, return success
        }
    }
}

/// Evaluate a [[ ]] conditional expression
fn evaluate_conditional(expr: &colony_parser::CondExpr, executor: &mut Executor) -> bool {
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

fn evaluate_arithmetic_command(expr: &str, executor: &mut Executor) -> i64 {
    match colony_expand::expand_arithmetic(expr, executor.context_mut()) {
        Ok(result) => result.parse().unwrap_or(0),
        Err(_) => 0,
    }
}

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

fn evaluate_cstyle_arith_expr(expr: &str, executor: &mut Executor) -> i64 {
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

fn evaluate_cstyle_arith_value(expr: &str, executor: &Executor) -> i64 {
    let expanded = expand_arithmetic_vars(expr, executor);
    evaluate_arithmetic_expr(&expanded)
}

fn get_var_as_i64(var: &str, executor: &Executor) -> i64 {
    executor.context().get_var_or_env(var)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

async fn execute_cstyle_for_loop(
    cstyle_for: &colony_parser::CStyleForStatement,
    executor: &mut Executor,
) -> i32 {
    if let Some(ref init) = cstyle_for.init {
        evaluate_cstyle_arith_expr(init, executor);
    }
    
    let mut last_exit = 0;
    
    loop {
        if let Some(ref cond) = cstyle_for.condition {
            let cond_result = evaluate_cstyle_arith_value(cond, executor);
            if cond_result == 0 {
                break;
            }
        }
        
        for stmt in &cstyle_for.body {
            if let Statement::SimpleCommand(cmd) = stmt {
                if let Some(name) = cmd.name.as_literal() {
                    match name {
                        "break" => return last_exit,
                        "continue" => {
                            if let Some(ref update) = cstyle_for.update {
                                evaluate_cstyle_arith_expr(update, executor);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            
            last_exit = execute_statement_for_code(stmt, executor).await;
        }
        
        if let Some(ref update) = cstyle_for.update {
            evaluate_cstyle_arith_expr(update, executor);
        }
    }
    
    last_exit
}

async fn execute_cstyle_for_loop_with_control(
    cstyle_for: &colony_parser::CStyleForStatement,
    executor: &mut Executor,
) -> i32 {
    if let Some(ref init) = cstyle_for.init {
        evaluate_cstyle_arith_expr(init, executor);
    }
    
    let mut last_exit = 0;
    
    loop {
        if let Some(ref cond) = cstyle_for.condition {
            let cond_result = evaluate_cstyle_arith_value(cond, executor);
            if cond_result == 0 {
                break;
            }
        }
        
        let mut should_break = false;
        let mut should_continue = false;
        
        for stmt in &cstyle_for.body {
            let (code, flow) = execute_statement_with_control(stmt, executor).await;
            last_exit = code;
            
            match flow {
                ControlFlow::Break => {
                    should_break = true;
                    break;
                }
                ControlFlow::Continue => {
                    should_continue = true;
                    break;
                }
                ControlFlow::Return | ControlFlow::Exit => {
                    should_break = true;
                    break;
                }
                ControlFlow::Normal => {}
            }
        }
        
        if should_break {
            break;
        }
        
        if let Some(ref update) = cstyle_for.update {
            evaluate_cstyle_arith_expr(update, executor);
        }
        
        if should_continue {
            continue;
        }
    }
    
    last_exit
}

/// Execute a for loop with break/continue support
async fn execute_for_loop(for_stmt: &colony_parser::ForStatement, executor: &mut Executor) -> i32 {
    use colony_expand::Expander;

    // Expand the word list (with command substitution support)
    let mut expander = Expander::with_executor(executor.context().clone(), |stmt, ctx| {
        colony_exec::execute_and_capture(stmt, ctx)
    });
    let mut values = Vec::new();

    for word in &for_stmt.words {
        match expander.expand_word(word) {
            Ok(fields) => {
                // Fields are the expanded words - just use them directly
                values.extend(fields);
            }
            Err(e) => {
                eprintln!("expansion error: {}", e);
                return 1;
            }
        }
    }

    // Iterate over values
    let mut last_exit = 0;
    for field in values {
        // Set loop variable (Field has .text)
        executor.context_mut().set_var(for_stmt.var.clone(), field.text);

        // Execute body
        for stmt in &for_stmt.body {
            // Check for break/continue
            if let Statement::SimpleCommand(cmd) = stmt {
                if let Some(name) = cmd.name.as_literal() {
                    match name {
                        "break" => return last_exit, // Exit loop immediately
                        "continue" => break,          // Skip to next iteration
                        _ => {}
                    }
                }
            }

            // Execute statement
            last_exit = execute_statement_for_code(stmt, executor).await;
        }
    }

    last_exit
}

/// Helper: Execute any statement and return (exit_code, control_flow)
fn execute_statement_with_control<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = (i32, ControlFlow)> + 'a>> {
    execute_statement_with_errexit_control(stmt, executor, false)
}

fn execute_statement_with_errexit_control<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
    errexit_exempt: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = (i32, ControlFlow)> + 'a>> {
    Box::pin(async move {
    match stmt {
        Statement::SimpleCommand(cmd) => {
            if let Some(literal) = cmd.name.as_literal() {
                match literal {
                    "break" => return (0, ControlFlow::Break),
                    "continue" => return (0, ControlFlow::Continue),
                    _ => {}
                }
            }

            let result = match executor.execute_simple_command(cmd).await {
                Ok(status) => {
                    let code = status.code();
                    if !errexit_exempt && executor.context().shell_options().errexit && code != 0 {
                        (code, ControlFlow::Exit)
                    } else {
                        (code, ControlFlow::Normal)
                    }
                }
                Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                Err(ExecutionError::Errexit(code)) => (code, ControlFlow::Exit),
                Err(ref e) => {
                    eprintln!("execution error: {}", e);
                    let exit_code = match e {
                        ExecutionError::CommandNotFound(_) => 127,
                        ExecutionError::PermissionDenied(_) => 126,
                        _ => 1,
                    };
                    if !errexit_exempt && executor.context().shell_options().errexit {
                        (exit_code, ControlFlow::Exit)
                    } else {
                        (exit_code, ControlFlow::Normal)
                    }
                }
            };
            if result.0 != 0 {
                execute_err_trap(executor).await;
            }
            result
        }
        Statement::Pipeline(pipeline) => {
            let pipeline_has_negation = pipeline.elements.first().map(|e| e.negated).unwrap_or(false);
            if pipeline.elements.len() == 1 {
                let element = &pipeline.elements[0];
                let saved_errexit = executor.context().shell_options().errexit;
                if element.negated {
                    executor.context_mut().shell_options_mut().errexit = false;
                }
                let (mut code, flow) = execute_statement_with_errexit_control(&element.command, executor, errexit_exempt || element.negated).await;
                if element.negated {
                    executor.context_mut().shell_options_mut().errexit = saved_errexit;
                    code = if code == 0 { 1 } else { 0 };
                }
                executor.context_mut().set_last_exit_status(code);
                return (code, flow);
            }
            let result = match executor.execute_pipeline(&pipeline).await {
                Ok(status) => {
                    let code = status.code();
                    if !errexit_exempt && !pipeline_has_negation && executor.context().shell_options().errexit && code != 0 {
                        (code, ControlFlow::Exit)
                    } else {
                        (code, ControlFlow::Normal)
                    }
                }
                Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                Err(ExecutionError::Errexit(code)) => (code, ControlFlow::Exit),
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    (1, ControlFlow::Normal)
                }
            };
            if result.0 != 0 && result.1 == ControlFlow::Normal {
                execute_err_trap(executor).await;
            }
            result
        }
        Statement::List(list) => {
            execute_list_with_control(list, executor).await
        }
        Statement::For(for_stmt) => {
            execute_for_loop_with_control(for_stmt, executor).await
        }
        Statement::CStyleFor(cstyle_for) => {
            (execute_cstyle_for_loop_with_control(cstyle_for, executor).await, ControlFlow::Normal)
        }
        Statement::If(if_stmt) => {
            execute_if_with_control(if_stmt, executor).await
        }
        Statement::While(_) | Statement::Until(_) | Statement::Case(_) => {
            match executor.execute_statement_recursive(stmt).await {
                Ok(code) => (code, ControlFlow::Normal),
                Err(ExecutionError::Break) => (0, ControlFlow::Break),
                Err(ExecutionError::Continue) => (0, ControlFlow::Continue),
                Err(ExecutionError::Exit(code)) => (code, ControlFlow::Exit),
                Err(ExecutionError::Errexit(code)) => (code, ControlFlow::Exit),
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    let exit_code = match &e {
                        ExecutionError::CommandNotFound(_) => 127,
                        ExecutionError::PermissionDenied(_) => 126,
                        _ => 1,
                    };
                    if !errexit_exempt && executor.context().shell_options().errexit {
                        (exit_code, ControlFlow::Exit)
                    } else {
                        (exit_code, ControlFlow::Normal)
                    }
                }
            }
        }
        Statement::BraceGroup(brace_group) => {
            use colony_exec::builtin_redirect::BuiltinRedirectGuard;
            let _redirect_guard = match BuiltinRedirectGuard::apply(&brace_group.redirects, executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return (1, ControlFlow::Normal);
                }
            };
            let mut last_exit = 0;
            for stmt in &brace_group.body {
                let (exit_code, flow) = execute_statement_with_control(stmt, executor).await;
                last_exit = exit_code;
                match flow {
                    ControlFlow::Normal => {}
                    other => return (last_exit, other),
                }
            }
            (last_exit, ControlFlow::Normal)
        }
        Statement::Subshell(subshell) => {
            use colony_exec::builtin_redirect::BuiltinRedirectGuard;
            let mut subshell_executor = Executor::new(executor.context().clone());
            subshell_executor.context_mut().clear_non_ignored_traps();
            let _redirect_guard = match BuiltinRedirectGuard::apply(&subshell.redirects, subshell_executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return (1, ControlFlow::Normal);
                }
            };
            let mut last_exit = 0;
            for stmt in &subshell.body {
                let (exit_code, flow) = execute_statement_with_control(stmt, &mut subshell_executor).await;
                last_exit = exit_code;
                match flow {
                    ControlFlow::Normal => {}
                    ControlFlow::Return | ControlFlow::Break | ControlFlow::Continue => {
                        return (last_exit, flow);
                    }
                    ControlFlow::Exit => return (last_exit, ControlFlow::Exit),
                }
            }
            subshell_executor.run_exit_trap().await;
            (last_exit, ControlFlow::Normal)
        }
        Statement::FunctionDef(func_def) => {
            // Store function definition
            executor.context_mut().define_function(func_def.name.clone(), func_def.clone());
            (0, ControlFlow::Normal)
        }
        Statement::Return(ret_stmt) => {
            // Return statement - propagate as control flow
            let exit_code = if let Some(ref _expr) = ret_stmt.value {
                // TODO: Evaluate expression
                0
            } else {
                0
            };
            (exit_code, ControlFlow::Return)
        }
        Statement::VarDecl(_) | Statement::ProcDef(_) => {
            eprintln!("This declaration not yet supported");
            (1, ControlFlow::Normal)
        }
        Statement::Conditional(cond_cmd) => {
            let result = evaluate_conditional(&cond_cmd.expr, executor);
            (if result { 0 } else { 1 }, ControlFlow::Normal)
        }
        Statement::Arithmetic(arith_cmd) => {
            let result = evaluate_arithmetic_command(&arith_cmd.expr, executor);
            (if result != 0 { 0 } else { 1 }, ControlFlow::Normal)
        }
        Statement::Coproc(coproc) => {
            match execute_coproc(coproc, executor).await {
                Ok(exit_code) => (exit_code, ControlFlow::Normal),
                Err(e) => {
                    eprintln!("colony: coproc error: {}", e);
                    (1, ControlFlow::Normal)
                }
            }
        }
        Statement::Empty => (executor.context().last_exit_status(), ControlFlow::Normal),
    }
    })
}

/// Helper for backwards compatibility: Execute and return only exit code
fn execute_statement_for_code<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + 'a>> {
    Box::pin(async move {
        let (code, _) = execute_statement_with_control(stmt, executor).await;
        executor.context_mut().set_last_exit_status(code);
        code
    })
}

/// Old implementation - kept for reference but now uses new version
#[allow(dead_code)]
fn _old_execute_statement_for_code<'a>(
    stmt: &'a Statement,
    executor: &'a mut Executor,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = i32> + 'a>> {
    Box::pin(async move {
    match stmt {
        Statement::SimpleCommand(cmd) => {
            match executor.execute_simple_command(cmd).await {
                Ok(status) => status.code(),
                Err(e) => {
                    eprintln!("execution error: {}", e);
                    1
                }
            }
        }
        Statement::Pipeline(pipeline) => {
            let element = &pipeline.elements[0];
            let is_negated = pipeline.elements.last().map(|e| e.negated).unwrap_or(false);
            
            if pipeline.elements.len() == 1 {
                let mut code = if let Statement::SimpleCommand(ref cmd) = *element.command {
                    match executor.execute_simple_command(cmd).await {
                        Ok(status) => status.code(),
                        Err(e) => {
                            eprintln!("execution error: {}", e);
                            1
                        }
                    }
                } else {
                    match executor.execute_pipeline(&pipeline).await {
                        Ok(status) => status.code(),
                        Err(e) => {
                            eprintln!("execution error: {}", e);
                            1
                        }
                    }
                };
                if is_negated {
                    code = if code == 0 { 1 } else { 0 };
                }
                code
            } else {
                let mut code = match executor.execute_pipeline(&pipeline).await {
                    Ok(status) => status.code(),
                    Err(e) => {
                        eprintln!("execution error: {}", e);
                        1
                    }
                };
                if is_negated {
                    code = if code == 0 { 1 } else { 0 };
                }
                code
            }
        }
        Statement::List(list) => {
            use colony_parser::ListOp;

            // Execute first statement
            let mut last_exit_code = execute_statement_for_code(&*list.first, executor).await;

            // Execute rest of list based on operators
            for (op, stmt) in &list.rest {
                let should_execute = match op {
                    ListOp::Semi => true,              // ; always executes
                    ListOp::And => last_exit_code == 0, // && executes if prev succeeded
                    ListOp::Or => last_exit_code != 0,  // || executes if prev failed
                    ListOp::Background => {
                        eprintln!("Background jobs (&) not yet supported");
                        false
                    }
                };

                if should_execute {
                    last_exit_code = execute_statement_for_code(stmt, executor).await;
                }
            }

            last_exit_code
        }
        Statement::For(for_stmt) => {
            execute_for_loop(for_stmt, executor).await
        }
        Statement::CStyleFor(cstyle_for) => {
            execute_cstyle_for_loop(cstyle_for, executor).await
        }
        Statement::If(if_stmt) => {
            execute_if_statement(if_stmt, executor).await
        }
        Statement::While(_) | Statement::Until(_) | Statement::Case(_) => {
            eprintln!("This control flow not yet supported");
            1
        }
        Statement::BraceGroup(brace_group) => {
            use colony_exec::builtin_redirect::BuiltinRedirectGuard;
            let _redirect_guard = match BuiltinRedirectGuard::apply(&brace_group.redirects, executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return 1;
                }
            };
            let mut last_exit = 0;
            for stmt in &brace_group.body {
                last_exit = Box::pin(execute_statement_for_code(stmt, executor)).await;
            }
            last_exit
        }
        Statement::Subshell(subshell) => {
            use colony_exec::builtin_redirect::BuiltinRedirectGuard;
            let _redirect_guard = match BuiltinRedirectGuard::apply(&subshell.redirects, executor.context_mut()) {
                Ok(guard) => guard,
                Err(e) => {
                    eprintln!("colony: redirect error: {}", e);
                    return 1;
                }
            };
            let mut last_exit = 0;
            for stmt in &subshell.body {
                last_exit = Box::pin(execute_statement_for_code(stmt, executor)).await;
            }
            last_exit
        }
        Statement::FunctionDef(func_def) => {
            // Store function definition
            executor.context_mut().define_function(func_def.name.clone(), func_def.clone());
            0
        }
        Statement::Return(_ret_stmt) => {
            // Return statement outside function - just return 0
            0
        }
        Statement::VarDecl(_) | Statement::ProcDef(_) => {
            eprintln!("This declaration not yet supported");
            1
        }
        Statement::Conditional(cond_cmd) => {
            if evaluate_conditional(&cond_cmd.expr, executor) { 0 } else { 1 }
        }
        Statement::Arithmetic(arith_cmd) => {
            if evaluate_arithmetic_command(&arith_cmd.expr, executor) != 0 { 0 } else { 1 }
        }
        Statement::Coproc(coproc) => {
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    handle.block_on(async {
                        execute_coproc(coproc, executor).await.unwrap_or(1)
                    })
                }
                Err(_) => {
                    eprintln!("colony: coproc requires async runtime");
                    1
                }
            }
        }
        Statement::Empty => 0,
    }
    })
}

/// Execute a shell command and return its exit code (for -c mode)
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
            if let Some(trap_code) = execute_exit_trap(executor, code).await {
                return trap_code;
            }
            return code;
        }
    }
    
    if let Some(trap_code) = execute_exit_trap(executor, last_exit).await {
        return trap_code;
    }
    last_exit
}

async fn execute_exit_trap(executor: &mut Executor, _exit_code: i32) -> Option<i32> {
    use colony_parser::pest_api::parse;
    
    if let Some(TrapAction::Command(cmd)) = executor.context().get_trap(&TrapSignal::Exit).cloned() {
        if let Ok(program) = parse(&cmd) {
            for stmt in &program.statements {
                let (code, flow) = execute_statement_with_control(&stmt, executor).await;
                if flow == ControlFlow::Exit {
                    return Some(code);
                }
            }
        }
    }
    None
}

async fn execute_err_trap(executor: &mut Executor) {
    use colony_parser::pest_api::parse;
    
    if let Some(TrapAction::Command(cmd)) = executor.context().get_trap(&TrapSignal::Err).cloned() {
        if let Ok(program) = parse(&cmd) {
            for stmt in &program.statements {
                let _ = execute_statement_for_code(&stmt, executor).await;
            }
        }
    }
}

/// Serialize a Statement to a shell command string for child process execution
fn statement_to_shell_string(stmt: &colony_parser::Statement) -> String {
    use colony_parser::{Statement, Word, WordPart, VariableRef};
    
    fn varref_to_string(v: &VariableRef) -> String {
        let mut result = String::from("$");
        if v.modifier.is_some() || v.subscript.is_some() || v.prefix.is_some() {
            result.push('{');
        }
        result.push_str(&v.name);
        if v.modifier.is_some() || v.subscript.is_some() || v.prefix.is_some() {
            result.push('}');
        }
        result
    }
    
    fn word_to_string(word: &Word) -> String {
        let mut result = String::new();
        for part in &word.parts {
            match part {
                WordPart::Literal(s) => {
                    for c in s.chars() {
                        if " \t\n\"'$`\\!&|;<>(){}[]".contains(c) {
                            result.push('\\');
                        }
                        result.push(c);
                    }
                }
                WordPart::SingleQuoted(s) => {
                    result.push('\'');
                    result.push_str(&s.replace('\'', "'\\''"));
                    result.push('\'');
                }
                WordPart::DoubleQuoted(parts) => {
                    result.push('"');
                    for p in parts {
                        match p {
                            WordPart::Literal(s) => {
                                for c in s.chars() {
                                    if "\"$`\\".contains(c) {
                                        result.push('\\');
                                    }
                                    result.push(c);
                                }
                            }
                            WordPart::Variable(v) => {
                                result.push_str(&varref_to_string(v));
                            }
                            _ => result.push_str(&format!("{:?}", p)),
                        }
                    }
                    result.push('"');
                }
                WordPart::Variable(v) => {
                    result.push_str(&varref_to_string(v));
                }
                WordPart::Tilde(user) => {
                    result.push('~');
                    if let Some(u) = user {
                        result.push_str(u);
                    }
                }
                WordPart::CommandSubRaw(s) => {
                    result.push_str("$(");
                    result.push_str(s);
                    result.push(')');
                }
                WordPart::ArithRaw(s) => {
                    result.push_str("$((");
                    result.push_str(s);
                    result.push_str("))");
                }
                _ => result.push_str(&format!("{:?}", part)),
            }
        }
        result
    }
    
    fn simple_cmd_to_string(cmd: &colony_parser::SimpleCommand) -> String {
        let mut parts = Vec::new();
        parts.push(word_to_string(&cmd.name));
        for arg in &cmd.args {
            parts.push(word_to_string(arg));
        }
        parts.join(" ")
    }
    
    fn statements_to_string(stmts: &[Statement]) -> String {
        stmts.iter()
            .map(statement_to_shell_string)
            .collect::<Vec<_>>()
            .join("; ")
    }
    
    fn condition_to_string(cond: &colony_parser::Condition) -> String {
        match cond {
            colony_parser::Condition::Command(stmt) => statement_to_shell_string(stmt),
            colony_parser::Condition::Expression(expr) => format!("{:?}", expr),
        }
    }
    
    match stmt {
        Statement::SimpleCommand(cmd) => simple_cmd_to_string(cmd),
        Statement::Pipeline(pipe) => {
            pipe.elements.iter()
                .map(|elem| statement_to_shell_string(&elem.command))
                .collect::<Vec<_>>()
                .join(" | ")
        }
        Statement::List(list) => {
            let mut result = statement_to_shell_string(&list.first);
            for (op, cmd) in &list.rest {
                let op_str = match op {
                    colony_parser::ListOp::And => "&&",
                    colony_parser::ListOp::Or => "||",
                    colony_parser::ListOp::Semi => ";",
                    colony_parser::ListOp::Background => "&",
                };
                result.push_str(&format!(" {} {}", op_str, statement_to_shell_string(cmd)));
            }
            result
        }
        Statement::BraceGroup(bg) => {
            format!("{{ {}; }}", statements_to_string(&bg.body))
        }
        Statement::Subshell(sub) => {
            format!("( {} )", statements_to_string(&sub.body))
        }
        Statement::If(if_stmt) => {
            let mut result = format!("if {}; then {}", 
                condition_to_string(&if_stmt.condition),
                statements_to_string(&if_stmt.then_body));
            for elif in &if_stmt.elif_branches {
                result.push_str(&format!("; elif {}; then {}",
                    condition_to_string(&elif.condition),
                    statements_to_string(&elif.body)));
            }
            if let Some(else_body) = &if_stmt.else_body {
                result.push_str(&format!("; else {}", statements_to_string(else_body)));
            }
            result.push_str("; fi");
            result
        }
        Statement::While(while_stmt) => {
            format!("while {}; do {}; done",
                condition_to_string(&while_stmt.condition),
                statements_to_string(&while_stmt.body))
        }
        Statement::Until(until_stmt) => {
            format!("until {}; do {}; done",
                condition_to_string(&until_stmt.condition),
                statements_to_string(&until_stmt.body))
        }
        Statement::For(for_stmt) => {
            let words: Vec<String> = for_stmt.words.iter()
                .map(word_to_string)
                .collect();
            format!("for {} in {}; do {}; done",
                for_stmt.var,
                words.join(" "),
                statements_to_string(&for_stmt.body))
        }
        Statement::Case(case_stmt) => {
            let word = word_to_string(&case_stmt.value);
            let mut result = format!("case {} in ", word);
            for arm in &case_stmt.arms {
                let patterns: Vec<String> = arm.patterns.iter()
                    .map(word_to_string)
                    .collect();
                result.push_str(&format!("{}) {};; ", patterns.join("|"), statements_to_string(&arm.body)));
            }
            result.push_str("esac");
            result
        }
        Statement::Empty => String::new(),
        other => format!("{:?}", other),
    }
}

async fn execute_coproc(coproc: &colony_parser::CoprocCommand, executor: &mut Executor) -> Result<i32, String> {
    use nix::sys::signal::{signal, SigHandler, Signal};
    use nix::unistd::{fork, ForkResult, pipe, dup2, close, execvp, setpgid};
    use std::ffi::CString;
    use std::os::fd::AsRawFd;

    let coproc_name = coproc.name.clone().unwrap_or_else(|| "COPROC".to_string());

    let stdin_pipe = pipe().map_err(|e| format!("pipe error: {}", e))?;
    let stdout_pipe = pipe().map_err(|e| format!("pipe error: {}", e))?;

    let is_simple_external = matches!(coproc.command.as_ref(), colony_parser::Statement::SimpleCommand(cmd)
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
        if let colony_parser::Statement::SimpleCommand(cmd) = coproc.command.as_ref() {
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

