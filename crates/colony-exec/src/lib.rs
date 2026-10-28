//! Command execution with fork/exec and job control
//!
//! This crate handles:
//! - Simple command execution (fork/exec)
//! - PATH resolution
//! - Environment passing
//! - Exit status capture
//! - Signal handling (future: job control)

mod alias;
pub mod buffers;
pub mod builtin_redirect;
mod builtins;
pub mod canvas_builtin;
mod command;
mod command_sub;
pub mod draw_builtin;
mod error;
pub mod events;
pub mod file_tree;
pub mod flux;
pub mod goals;
pub mod heartbeat;
pub mod history;
mod jobs;
pub mod mirror;
pub mod panes;
mod path;
mod pipeline;
mod process;
mod redirect;
pub mod serve;
pub mod sidebar;
pub mod special_files;
mod xtrace;

#[cfg(test)]
mod export_func_tests;
#[cfg(test)]
pub(crate) mod test_helpers;

pub use alias::{expand_alias, AliasExpansionResult};

pub use builtins::{execute_builtin, is_builtin, BuiltinResult};
pub use command::execute_command;
pub use command_sub::{execute_and_capture, execute_statement_for_process_sub};
pub use error::{ExecutionError, Result};
pub use jobs::{
    continue_job, give_terminal_to, init_job_control, reap_children, take_terminal,
    wait_for_foreground_job, Job, JobId, JobStatus, JobTable,
};
pub use path::resolve_command;
pub use pipeline::execute_pipeline;
pub use process::{ExitStatus, ProcessHandle};
pub use redirect::{
    apply_channel_redirects_in_child, apply_redirects, is_channel_fd, prepare_channel_redirects,
    process_channel_writes, ChannelPipe, STDCONTEXT, STDDIAG, STDLLM, STDTOOL,
};

use colony_channels::ChannelManager;
use colony_expand::{ExpansionContext, VarAttributes, Variable};
use colony_parser::{Pipeline, SimpleCommand};
use colony_reflection::ReflectionObserver;
use colony_tuple::TupleSpace;

use std::sync::Arc;

pub use history::History;

pub fn build_env(context: &ExpansionContext) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = std::env::vars().collect();

    for (name, value) in context.exported_env() {
        env.retain(|(k, _)| k != &name);
        env.push((name, value));
    }

    for func_name in context.exported_function_names() {
        if let Some(func) = context.get_function(func_name) {
            let body = builtins::serialize_function_for_export(func);
            let env_name = format!("BASH_FUNC_{}%%", func_name);
            env.retain(|(k, _)| k != &env_name);
            env.push((env_name, body));
        }
    }

    env
}

#[derive(Debug, Clone)]
pub(crate) enum EvalValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<EvalValue>),
    Dict(Vec<(EvalValue, EvalValue)>),
    Tuple(Vec<EvalValue>),
    Optional(Box<EvalValue>),
}

fn eval_binary(
    op: &colony_parser::BinaryOp,
    left: EvalValue,
    right: EvalValue,
) -> Result<EvalValue> {
    use colony_parser::BinaryOp;

    let left = unwrap_optional(left);
    let right = unwrap_optional(right);

    match op {
        BinaryOp::Add => match (left, right) {
            (EvalValue::Str(l), EvalValue::Str(r)) => Ok(EvalValue::Str(format!("{}{}", l, r))),
            (EvalValue::Str(l), EvalValue::Int(r)) => {
                if let Ok(n) = l.trim().parse::<i64>() {
                    Ok(EvalValue::Int(n + r))
                } else if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n + r as f64))
                } else {
                    Ok(EvalValue::Str(format!("{}{}", l, r)))
                }
            }
            (EvalValue::Int(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<i64>() {
                    Ok(EvalValue::Int(l + n))
                } else if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l as f64 + n))
                } else {
                    Ok(EvalValue::Str(format!("{}{}", l, r)))
                }
            }
            (EvalValue::Str(l), EvalValue::Float(r)) => {
                if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n + r))
                } else {
                    Ok(EvalValue::Str(format!("{}{}", l, r)))
                }
            }
            (EvalValue::Float(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l + n))
                } else {
                    Ok(EvalValue::Str(format!("{}{}", l, r)))
                }
            }
            (EvalValue::Int(l), EvalValue::Int(r)) => Ok(EvalValue::Int(l + r)),
            (EvalValue::Float(l), EvalValue::Float(r)) => Ok(EvalValue::Float(l + r)),
            (EvalValue::Int(l), EvalValue::Float(r)) => Ok(EvalValue::Float(l as f64 + r)),
            (EvalValue::Float(l), EvalValue::Int(r)) => Ok(EvalValue::Float(l + r as f64)),
            _ => Err(ExecutionError::InvalidVarDecl(
                "unsupported + operands in var declaration".to_string(),
            )),
        },
        BinaryOp::Sub => match (left, right) {
            (EvalValue::Str(l), EvalValue::Int(r)) => {
                if let Ok(n) = l.trim().parse::<i64>() {
                    Ok(EvalValue::Int(n - r))
                } else if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n - r as f64))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported - operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<i64>() {
                    Ok(EvalValue::Int(l - n))
                } else if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l as f64 - n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported - operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Str(l), EvalValue::Float(r)) => {
                if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n - r))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported - operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Float(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l - n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported - operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Int(r)) => Ok(EvalValue::Int(l - r)),
            (EvalValue::Float(l), EvalValue::Float(r)) => Ok(EvalValue::Float(l - r)),
            (EvalValue::Int(l), EvalValue::Float(r)) => Ok(EvalValue::Float(l as f64 - r)),
            (EvalValue::Float(l), EvalValue::Int(r)) => Ok(EvalValue::Float(l - r as f64)),
            _ => Err(ExecutionError::InvalidVarDecl(
                "unsupported - operands in var declaration".to_string(),
            )),
        },
        BinaryOp::Mul => match (left, right) {
            (EvalValue::Str(l), EvalValue::Int(r)) => {
                if let Ok(n) = l.trim().parse::<i64>() {
                    Ok(EvalValue::Int(n * r))
                } else if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n * r as f64))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported * operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<i64>() {
                    Ok(EvalValue::Int(l * n))
                } else if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l as f64 * n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported * operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Str(l), EvalValue::Float(r)) => {
                if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n * r))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported * operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Float(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l * n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported * operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Int(r)) => Ok(EvalValue::Int(l * r)),
            (EvalValue::Float(l), EvalValue::Float(r)) => Ok(EvalValue::Float(l * r)),
            (EvalValue::Int(l), EvalValue::Float(r)) => Ok(EvalValue::Float(l as f64 * r)),
            (EvalValue::Float(l), EvalValue::Int(r)) => Ok(EvalValue::Float(l * r as f64)),
            _ => Err(ExecutionError::InvalidVarDecl(
                "unsupported * operands in var declaration".to_string(),
            )),
        },
        BinaryOp::Div => match (left, right) {
            (EvalValue::Int(_), EvalValue::Int(0))
            | (EvalValue::Float(_), EvalValue::Float(0.0))
            | (EvalValue::Int(_), EvalValue::Float(0.0))
            | (EvalValue::Float(_), EvalValue::Int(0)) => Err(ExecutionError::InvalidVarDecl(
                "division by zero in var declaration".to_string(),
            )),
            (EvalValue::Str(l), EvalValue::Int(r)) => {
                if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n / r as f64))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported / operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l as f64 / n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported / operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Str(l), EvalValue::Float(r)) => {
                if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n / r))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported / operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Float(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l / n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported / operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Int(r)) => Ok(EvalValue::Float(l as f64 / r as f64)),
            (EvalValue::Float(l), EvalValue::Float(r)) => Ok(EvalValue::Float(l / r)),
            (EvalValue::Int(l), EvalValue::Float(r)) => Ok(EvalValue::Float(l as f64 / r)),
            (EvalValue::Float(l), EvalValue::Int(r)) => Ok(EvalValue::Float(l / r as f64)),
            _ => Err(ExecutionError::InvalidVarDecl(
                "unsupported / operands in var declaration".to_string(),
            )),
        },
        BinaryOp::IntDiv => match (left, right) {
            (EvalValue::Int(_), EvalValue::Int(0)) => Err(ExecutionError::InvalidVarDecl(
                "division by zero in var declaration".to_string(),
            )),
            (EvalValue::Str(l), EvalValue::Int(r)) => {
                if let Ok(n) = l.trim().parse::<i64>() {
                    Ok(EvalValue::Int(n / r))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported // operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<i64>() {
                    Ok(EvalValue::Int(l / n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported // operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Str(l), EvalValue::Float(r)) => {
                if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Int((n / r) as i64))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported // operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Float(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Int((l / n) as i64))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported // operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Int(r)) => Ok(EvalValue::Int(l / r)),
            _ => Err(ExecutionError::InvalidVarDecl(
                "unsupported // operands in var declaration".to_string(),
            )),
        },
        BinaryOp::Mod => match (left, right) {
            (EvalValue::Int(_), EvalValue::Int(0)) => Err(ExecutionError::InvalidVarDecl(
                "modulo by zero in var declaration".to_string(),
            )),
            (EvalValue::Str(l), EvalValue::Int(r)) => {
                if let Ok(n) = l.trim().parse::<i64>() {
                    Ok(EvalValue::Int(n % r))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported % operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<i64>() {
                    Ok(EvalValue::Int(l % n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported % operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Str(l), EvalValue::Float(r)) => {
                if let Ok(n) = l.trim().parse::<f64>() {
                    Ok(EvalValue::Float(n % r))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported % operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Float(l), EvalValue::Str(r)) => {
                if let Ok(n) = r.trim().parse::<f64>() {
                    Ok(EvalValue::Float(l % n))
                } else {
                    Err(ExecutionError::InvalidVarDecl(
                        "unsupported % operands in var declaration".to_string(),
                    ))
                }
            }
            (EvalValue::Int(l), EvalValue::Int(r)) => Ok(EvalValue::Int(l % r)),
            _ => Err(ExecutionError::InvalidVarDecl(
                "unsupported % operands in var declaration".to_string(),
            )),
        },
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            let result = match (left, right) {
                (EvalValue::Int(l), EvalValue::Int(r)) => cmp_numbers(op, l as f64, r as f64),
                (EvalValue::Float(l), EvalValue::Float(r)) => cmp_numbers(op, l, r),
                (EvalValue::Int(l), EvalValue::Float(r)) => cmp_numbers(op, l as f64, r),
                (EvalValue::Float(l), EvalValue::Int(r)) => cmp_numbers(op, l, r as f64),
                (EvalValue::Str(l), EvalValue::Str(r)) => cmp_strings(op, &l, &r),
                _ => {
                    return Err(ExecutionError::InvalidVarDecl(
                        "unsupported comparison in var declaration".to_string(),
                    ))
                }
            };
            Ok(EvalValue::Bool(result))
        }
        BinaryOp::And | BinaryOp::Or => match (left, right) {
            (EvalValue::Bool(l), EvalValue::Bool(r)) => Ok(EvalValue::Bool(match op {
                BinaryOp::And => l && r,
                BinaryOp::Or => l || r,
                _ => false,
            })),
            _ => Err(ExecutionError::InvalidVarDecl(
                "unsupported logical operands in var declaration".to_string(),
            )),
        },
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr => {
            match (left, right) {
                (EvalValue::Int(l), EvalValue::Int(r)) => Ok(EvalValue::Int(match op {
                    BinaryOp::BitAnd => l & r,
                    BinaryOp::BitOr => l | r,
                    BinaryOp::BitXor => l ^ r,
                    BinaryOp::Shl => l << r,
                    BinaryOp::Shr => l >> r,
                    _ => l,
                })),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "unsupported bitwise operands in var declaration".to_string(),
                )),
            }
        }
        BinaryOp::Match | BinaryOp::NotMatch | BinaryOp::Pow => Err(
            ExecutionError::InvalidVarDecl("unsupported operator in var declaration".to_string()),
        ),
    }
}

pub(crate) fn eval_value_to_variable(value: EvalValue) -> Result<Variable> {
    match value {
        EvalValue::Null => Ok(Variable::String(String::new())),
        EvalValue::Bool(b) => Ok(Variable::String(b.to_string())),
        EvalValue::Int(n) => Ok(Variable::String(n.to_string())),
        EvalValue::Float(f) => Ok(Variable::String(f.to_string())),
        EvalValue::Str(s) => Ok(Variable::String(s)),
        EvalValue::List(items) | EvalValue::Tuple(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(eval_value_to_string(item));
            }
            Ok(Variable::Array(out))
        }
        EvalValue::Dict(entries) => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in entries {
                map.insert(eval_value_to_string(k), eval_value_to_string(v));
            }
            Ok(Variable::AssocArray(map))
        }
        EvalValue::Optional(inner) => eval_value_to_variable(*inner),
    }
}

pub(crate) fn eval_value_to_string(value: EvalValue) -> String {
    match value {
        EvalValue::Null => String::new(),
        EvalValue::Bool(b) => b.to_string(),
        EvalValue::Int(n) => n.to_string(),
        EvalValue::Float(f) => f.to_string(),
        EvalValue::Str(s) => s,
        EvalValue::List(items) | EvalValue::Tuple(items) => {
            let parts: Vec<String> = items.into_iter().map(eval_value_to_string).collect();
            format!("[{}]", parts.join(", "))
        }
        EvalValue::Dict(entries) => {
            let parts: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| format!("{}: {}", eval_value_to_string(k), eval_value_to_string(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        EvalValue::Optional(inner) => eval_value_to_string(*inner),
    }
}

fn eval_value_from_variable(var: &Variable) -> EvalValue {
    match var {
        Variable::String(s) => EvalValue::Str(s.clone()),
        Variable::Array(values) => {
            EvalValue::List(values.iter().cloned().map(EvalValue::Str).collect())
        }
        Variable::AssocArray(map) => EvalValue::Dict(
            map.iter()
                .map(|(k, v)| (EvalValue::Str(k.clone()), EvalValue::Str(v.clone())))
                .collect(),
        ),
        Variable::NameRef(target) => EvalValue::Str(target.clone()),
    }
}

fn error_type_name(err: &ExecutionError) -> &'static str {
    match err {
        ExecutionError::CommandNotFound(_) => "CommandNotFound",
        ExecutionError::PermissionDenied(_) => "PermissionDenied",
        ExecutionError::ExpansionError(_) => "ExpansionError",
        ExecutionError::ForkError(_) => "ForkError",
        ExecutionError::ExecError(_) => "ExecError",
        ExecutionError::WaitError(_) => "WaitError",
        ExecutionError::IoError(_) => "IoError",
        ExecutionError::InvalidUtf8 => "InvalidUtf8",
        ExecutionError::RedirectError(_) => "RedirectError",
        ExecutionError::AmbiguousRedirect => "AmbiguousRedirect",
        ExecutionError::InvalidRedirect(_) => "InvalidRedirect",
        ExecutionError::InvalidVarDecl(_) => "InvalidVarDecl",
        ExecutionError::InvalidPipeline(_) => "InvalidPipeline",
        ExecutionError::Return(_) => "Return",
        ExecutionError::Break => "Break",
        ExecutionError::Continue => "Continue",
        ExecutionError::Exit(_) => "Exit",
        ExecutionError::Errexit(_) => "Errexit",
    }
}

fn error_code(err: &ExecutionError) -> i32 {
    match err {
        ExecutionError::CommandNotFound(_) => 127,
        ExecutionError::PermissionDenied(_) => 126,
        ExecutionError::Return(code) => *code,
        ExecutionError::Exit(code) => *code,
        ExecutionError::Errexit(code) => *code,
        _ => 1,
    }
}

/// Evaluate an expression against the current expansion context, returning a string.
pub fn eval_expression_to_string(
    expr: &colony_parser::Expression,
    context: &mut ExpansionContext,
) -> Result<String> {
    let value = eval_expression_in_context(context, expr)?;
    Ok(eval_value_to_string(value))
}

pub(crate) fn eval_expression_in_context(
    context: &mut ExpansionContext,
    expr: &colony_parser::Expression,
) -> Result<EvalValue> {
    use colony_parser::{Expression, UnaryOp};

    match expr {
        Expression::Null(_) => Ok(EvalValue::Null),
        Expression::Bool(b, _) => Ok(EvalValue::Bool(*b)),
        Expression::Int(n, _) => Ok(EvalValue::Int(*n)),
        Expression::Float(f, _) => Ok(EvalValue::Float(*f)),
        Expression::Str(s, _) => Ok(EvalValue::Str(s.clone())),
        Expression::Variable { name, .. } => match context.get_var_or_env(name) {
            Some(value) => {
                if let Some(var) = context.get_variable(name) {
                    Ok(eval_value_from_variable(var))
                } else {
                    Ok(EvalValue::Str(value))
                }
            }
            None => Ok(EvalValue::Null),
        },
        Expression::Paren { inner, .. } => eval_expression_in_context(context, inner),
        Expression::List { items, .. } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(eval_expression_in_context(context, item)?);
            }
            Ok(EvalValue::List(values))
        }
        Expression::Dict { entries, .. } => {
            let mut values = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                values.push((
                    eval_expression_in_context(context, key)?,
                    eval_expression_in_context(context, value)?,
                ));
            }
            Ok(EvalValue::Dict(values))
        }
        Expression::Tuple { items, .. } => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(eval_expression_in_context(context, item)?);
            }
            Ok(EvalValue::Tuple(values))
        }
        Expression::Index { target, index, .. } => {
            let target_val = eval_expression_in_context(context, target)?;
            let index_val = eval_expression_in_context(context, index)?;
            eval_index(target_val, index_val)
        }
        Expression::Field { target, field, .. } => {
            let target_val = eval_expression_in_context(context, target)?;
            eval_field(target_val, field)
        }
        Expression::CommandSub { command, .. } => {
            let output =
                execute_and_capture(command, context).map_err(ExecutionError::InvalidVarDecl)?;
            Ok(EvalValue::Str(output.trim_end_matches('\n').to_string()))
        }
        Expression::Unary { op, operand, .. } => {
            let val = eval_expression_in_context(context, operand)?;
            match (op, val) {
                (UnaryOp::Neg, EvalValue::Int(n)) => Ok(EvalValue::Int(-n)),
                (UnaryOp::Neg, EvalValue::Float(f)) => Ok(EvalValue::Float(-f)),
                (UnaryOp::Not, EvalValue::Bool(b)) => Ok(EvalValue::Bool(!b)),
                (UnaryOp::BitNot, EvalValue::Int(n)) => Ok(EvalValue::Int(!n)),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "unsupported unary expression in var declaration".to_string(),
                )),
            }
        }
        Expression::Binary {
            left, op, right, ..
        } => {
            let l = eval_expression_in_context(context, left)?;
            let r = eval_expression_in_context(context, right)?;
            eval_binary(op, l, r)
        }
        Expression::FuncCall { name, args, .. } => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval_expression_in_context(context, arg)?);
            }
            eval_func_call(name, &values)
        }
        Expression::OptionalCall { name, args, .. } => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval_expression_in_context(context, arg)?);
            }
            let result = eval_func_call(name, &values)?;
            Ok(EvalValue::Optional(Box::new(result)))
        }
        Expression::MethodCall {
            target, name, args, ..
        } => {
            let target_val = eval_expression_in_context(context, target)?;
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(eval_expression_in_context(context, arg)?);
            }
            eval_method_call(target_val, name, &values)
        }
        Expression::OptionalChain { target, .. } => {
            let value = eval_expression_in_context(context, target)?;
            match value {
                EvalValue::Optional(_) => Ok(value),
                other => Ok(EvalValue::Optional(Box::new(other))),
            }
        }
        Expression::Unwrap { target, .. } => {
            let value = eval_expression_in_context(context, target)?;
            let value = unwrap_optional(value);
            if is_nullish(&value) {
                Err(ExecutionError::InvalidVarDecl(
                    "unwrap on null in var declaration".to_string(),
                ))
            } else {
                Ok(value)
            }
        }
        Expression::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            let cond_val = eval_expression_in_context(context, condition)?;
            if is_truthy(&cond_val) {
                eval_expression_in_context(context, then_expr)
            } else {
                eval_expression_in_context(context, else_expr)
            }
        }
        Expression::NullCoalesce { left, right, .. } => {
            let left_val = eval_expression_in_context(context, left)?;
            if is_nullish(&left_val) {
                eval_expression_in_context(context, right)
            } else {
                Ok(left_val)
            }
        }
        _ => Err(ExecutionError::InvalidVarDecl(
            "unsupported expression in var declaration".to_string(),
        )),
    }
}

pub(crate) fn unwrap_optional(value: EvalValue) -> EvalValue {
    match value {
        EvalValue::Optional(inner) => *inner,
        other => other,
    }
}

fn maybe_optional_wrap(original: EvalValue, value: EvalValue) -> EvalValue {
    if matches!(original, EvalValue::Optional(_)) {
        EvalValue::Optional(Box::new(value))
    } else {
        value
    }
}

fn eval_index(target: EvalValue, index: EvalValue) -> Result<EvalValue> {
    let target_optional = matches!(target, EvalValue::Optional(_));
    let target = unwrap_optional(target);
    let index = unwrap_optional(index);

    let result = match (target, index) {
        (EvalValue::List(items), EvalValue::Int(i)) => {
            Ok(items.get(i as usize).cloned().unwrap_or(EvalValue::Null))
        }
        (EvalValue::Tuple(items), EvalValue::Int(i)) => {
            Ok(items.get(i as usize).cloned().unwrap_or(EvalValue::Null))
        }
        (EvalValue::Dict(entries), key) => {
            let key_str = eval_value_to_string(key);
            for (k, v) in entries {
                if eval_value_to_string(k) == key_str {
                    return Ok(v);
                }
            }
            Ok(EvalValue::Null)
        }
        (EvalValue::Str(s), EvalValue::Int(i)) => {
            let idx = i as usize;
            Ok(s.chars()
                .nth(idx)
                .map(|ch| EvalValue::Str(ch.to_string()))
                .unwrap_or(EvalValue::Null))
        }
        (EvalValue::Null, _) => Ok(EvalValue::Null),
        _ => Err(ExecutionError::InvalidVarDecl(
            "unsupported index expression in var declaration".to_string(),
        )),
    }?;

    if target_optional {
        Ok(maybe_optional_wrap(
            EvalValue::Optional(Box::new(EvalValue::Null)),
            result,
        ))
    } else {
        Ok(result)
    }
}

fn eval_field(target: EvalValue, field: &str) -> Result<EvalValue> {
    let target_optional = matches!(target, EvalValue::Optional(_));
    let target = unwrap_optional(target);

    let result = match target {
        EvalValue::Dict(entries) => {
            for (k, v) in entries {
                if eval_value_to_string(k) == field {
                    return Ok(v);
                }
            }
            Ok(EvalValue::Null)
        }
        EvalValue::List(items) | EvalValue::Tuple(items) if field == "len" => {
            Ok(EvalValue::Int(items.len() as i64))
        }
        EvalValue::Str(s) if field == "len" => Ok(EvalValue::Int(s.chars().count() as i64)),
        EvalValue::Null => Ok(EvalValue::Null),
        _ => Err(ExecutionError::InvalidVarDecl(
            "unsupported field access in var declaration".to_string(),
        )),
    }?;

    if target_optional {
        Ok(maybe_optional_wrap(
            EvalValue::Optional(Box::new(EvalValue::Null)),
            result,
        ))
    } else {
        Ok(result)
    }
}

fn eval_func_call(name: &str, args: &[EvalValue]) -> Result<EvalValue> {
    match name {
        "len" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("len() expects 1 argument".to_string())
            })?;
            let arg = unwrap_optional(arg.clone());
            match arg {
                EvalValue::Str(s) => Ok(EvalValue::Int(s.chars().count() as i64)),
                EvalValue::List(items) => Ok(EvalValue::Int(items.len() as i64)),
                EvalValue::Tuple(items) => Ok(EvalValue::Int(items.len() as i64)),
                EvalValue::Dict(entries) => Ok(EvalValue::Int(entries.len() as i64)),
                EvalValue::Null => Ok(EvalValue::Int(0)),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "len() unsupported argument type".to_string(),
                )),
            }
        }
        "upper" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("upper() expects 1 argument".to_string())
            })?;
            Ok(EvalValue::Str(
                eval_value_to_string(unwrap_optional(arg.clone())).to_uppercase(),
            ))
        }
        "lower" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("lower() expects 1 argument".to_string())
            })?;
            Ok(EvalValue::Str(
                eval_value_to_string(unwrap_optional(arg.clone())).to_lowercase(),
            ))
        }
        "int" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("int() expects 1 argument".to_string())
            })?;
            let s = eval_value_to_string(unwrap_optional(arg.clone()));
            let parsed = s
                .trim()
                .parse::<i64>()
                .map_err(|_| ExecutionError::InvalidVarDecl("int() parse error".to_string()))?;
            Ok(EvalValue::Int(parsed))
        }
        "float" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("float() expects 1 argument".to_string())
            })?;
            let s = eval_value_to_string(unwrap_optional(arg.clone()));
            let parsed = s
                .trim()
                .parse::<f64>()
                .map_err(|_| ExecutionError::InvalidVarDecl("float() parse error".to_string()))?;
            Ok(EvalValue::Float(parsed))
        }
        "str" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("str() expects 1 argument".to_string())
            })?;
            Ok(EvalValue::Str(eval_value_to_string(unwrap_optional(
                arg.clone(),
            ))))
        }
        "bool" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("bool() expects 1 argument".to_string())
            })?;
            Ok(EvalValue::Bool(is_truthy(&unwrap_optional(arg.clone()))))
        }
        "join" => {
            let sep = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("join() expects 2 arguments".to_string())
            })?;
            let items = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("join() expects 2 arguments".to_string())
            })?;
            let sep_str = eval_value_to_string(unwrap_optional(sep.clone()));
            let items = unwrap_optional(items.clone());
            match items {
                EvalValue::List(values) | EvalValue::Tuple(values) => {
                    let parts: Vec<String> =
                        values.iter().cloned().map(eval_value_to_string).collect();
                    Ok(EvalValue::Str(parts.join(&sep_str)))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "join() expects list/tuple as second argument".to_string(),
                )),
            }
        }
        "split" => {
            let sep = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("split() expects 2 arguments".to_string())
            })?;
            let input = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("split() expects 2 arguments".to_string())
            })?;
            let sep_str = eval_value_to_string(unwrap_optional(sep.clone()));
            let input_str = eval_value_to_string(unwrap_optional(input.clone()));
            let parts: Vec<String> = if sep_str.is_empty() {
                input_str.chars().map(|c| c.to_string()).collect()
            } else {
                input_str.split(&sep_str).map(|s| s.to_string()).collect()
            };
            Ok(EvalValue::List(
                parts.into_iter().map(EvalValue::Str).collect(),
            ))
        }
        "keys" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("keys() expects 1 argument".to_string())
            })?;
            let arg = unwrap_optional(arg.clone());
            match arg {
                EvalValue::Dict(entries) => {
                    let keys = entries.iter().map(|(k, _)| k.clone()).collect();
                    Ok(EvalValue::List(keys))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "keys() expects dict argument".to_string(),
                )),
            }
        }
        "values" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("values() expects 1 argument".to_string())
            })?;
            let arg = unwrap_optional(arg.clone());
            match arg {
                EvalValue::Dict(entries) => {
                    let values = entries.iter().map(|(_, v)| v.clone()).collect();
                    Ok(EvalValue::List(values))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "values() expects dict argument".to_string(),
                )),
            }
        }
        "get" => {
            let dict = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("get() expects at least 2 arguments".to_string())
            })?;
            let key = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("get() expects at least 2 arguments".to_string())
            })?;
            let default = args.get(2).cloned().unwrap_or(EvalValue::Null);
            let dict = unwrap_optional(dict.clone());
            let key = unwrap_optional(key.clone());
            match dict {
                EvalValue::Dict(entries) => Ok(dict_get(entries, key).unwrap_or(default)),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "get() expects dict as first argument".to_string(),
                )),
            }
        }
        "has" => {
            let dict = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("has() expects 2 arguments".to_string())
            })?;
            let key = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("has() expects 2 arguments".to_string())
            })?;
            let dict = unwrap_optional(dict.clone());
            let key = unwrap_optional(key.clone());
            match dict {
                EvalValue::Dict(entries) => Ok(EvalValue::Bool(dict_has(entries, key))),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "has() expects dict as first argument".to_string(),
                )),
            }
        }
        "trim" => {
            let arg = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("trim() expects 1 argument".to_string())
            })?;
            let s = eval_value_to_string(unwrap_optional(arg.clone()));
            Ok(EvalValue::Str(s.trim().to_string()))
        }
        "replace" => {
            let input = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("replace() expects 3 arguments".to_string())
            })?;
            let old = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("replace() expects 3 arguments".to_string())
            })?;
            let new = args.get(2).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("replace() expects 3 arguments".to_string())
            })?;
            let input = eval_value_to_string(unwrap_optional(input.clone()));
            let old = eval_value_to_string(unwrap_optional(old.clone()));
            let new = eval_value_to_string(unwrap_optional(new.clone()));
            Ok(EvalValue::Str(input.replace(&old, &new)))
        }
        "contains" => {
            let input = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("contains() expects 2 arguments".to_string())
            })?;
            let needle = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("contains() expects 2 arguments".to_string())
            })?;
            let input = eval_value_to_string(unwrap_optional(input.clone()));
            let needle = eval_value_to_string(unwrap_optional(needle.clone()));
            Ok(EvalValue::Bool(input.contains(&needle)))
        }
        "first" => {
            let list = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("first() expects 1 argument".to_string())
            })?;
            let list = unwrap_optional(list.clone());
            match list {
                EvalValue::List(items) | EvalValue::Tuple(items) => {
                    Ok(items.into_iter().next().unwrap_or(EvalValue::Null))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "first() expects list/tuple argument".to_string(),
                )),
            }
        }
        "last" => {
            let list = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("last() expects 1 argument".to_string())
            })?;
            let list = unwrap_optional(list.clone());
            match list {
                EvalValue::List(items) | EvalValue::Tuple(items) => {
                    Ok(items.into_iter().last().unwrap_or(EvalValue::Null))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "last() expects list/tuple argument".to_string(),
                )),
            }
        }
        "push" => {
            let list = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("push() expects 2 arguments".to_string())
            })?;
            let item = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("push() expects 2 arguments".to_string())
            })?;
            let list = unwrap_optional(list.clone());
            let item = unwrap_optional(item.clone());
            match list {
                EvalValue::List(mut items) => {
                    items.push(item);
                    Ok(EvalValue::List(items))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "push() expects list as first argument".to_string(),
                )),
            }
        }
        "pop" => {
            let list = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("pop() expects 1 argument".to_string())
            })?;
            let list = unwrap_optional(list.clone());
            match list {
                EvalValue::List(mut items) => {
                    let value = items.pop().unwrap_or(EvalValue::Null);
                    Ok(EvalValue::Tuple(vec![EvalValue::List(items), value]))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "pop() expects list as first argument".to_string(),
                )),
            }
        }
        "insert" => {
            let list = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("insert() expects 3 arguments".to_string())
            })?;
            let index = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("insert() expects 3 arguments".to_string())
            })?;
            let value = args.get(2).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("insert() expects 3 arguments".to_string())
            })?;
            let list = unwrap_optional(list.clone());
            let index = unwrap_optional(index.clone());
            let value = unwrap_optional(value.clone());
            match list {
                EvalValue::List(mut items) => {
                    let idx = eval_index_as_usize(index)?;
                    let idx = idx.min(items.len());
                    items.insert(idx, value);
                    Ok(EvalValue::List(items))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "insert() expects list as first argument".to_string(),
                )),
            }
        }
        "remove" => {
            let list = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("remove() expects 2 arguments".to_string())
            })?;
            let index = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("remove() expects 2 arguments".to_string())
            })?;
            let list = unwrap_optional(list.clone());
            let index = unwrap_optional(index.clone());
            match list {
                EvalValue::List(mut items) => {
                    let idx = eval_index_as_usize(index)?;
                    let value = if idx < items.len() {
                        items.remove(idx)
                    } else {
                        EvalValue::Null
                    };
                    Ok(EvalValue::Tuple(vec![EvalValue::List(items), value]))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "remove() expects list as first argument".to_string(),
                )),
            }
        }
        "slice" => {
            let list = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("slice() expects 2 or 3 arguments".to_string())
            })?;
            let start = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("slice() expects 2 or 3 arguments".to_string())
            })?;
            let end = args.get(2).cloned();
            let list = unwrap_optional(list.clone());
            let start = unwrap_optional(start.clone());
            let end = end.map(unwrap_optional);
            match list {
                EvalValue::List(items) => {
                    let s = eval_index_as_usize(start)?;
                    let e = match end {
                        Some(v) => eval_index_as_usize(v)?,
                        None => items.len(),
                    };
                    let s = s.min(items.len());
                    let e = e.min(items.len());
                    let out = if s >= e {
                        Vec::new()
                    } else {
                        items[s..e].to_vec()
                    };
                    Ok(EvalValue::List(out))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "slice() expects list as first argument".to_string(),
                )),
            }
        }
        "contains_str" => {
            let input = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("contains_str() expects 2 arguments".to_string())
            })?;
            let needle = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("contains_str() expects 2 arguments".to_string())
            })?;
            let input = eval_value_to_string(unwrap_optional(input.clone()));
            let needle = eval_value_to_string(unwrap_optional(needle.clone()));
            Ok(EvalValue::Bool(input.contains(&needle)))
        }
        _ => Err(ExecutionError::InvalidVarDecl(format!(
            "unknown function in var declaration: {}",
            name
        ))),
    }
}

fn eval_method_call(target: EvalValue, name: &str, args: &[EvalValue]) -> Result<EvalValue> {
    let target_optional = matches!(target, EvalValue::Optional(_));
    let target = unwrap_optional(target);

    if matches!(target, EvalValue::Null) && target_optional {
        return Ok(EvalValue::Optional(Box::new(EvalValue::Null)));
    }

    let result = match name {
        "len" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "len() expects 0 arguments".to_string(),
                ));
            }
            match target {
                EvalValue::Str(s) => Ok(EvalValue::Int(s.chars().count() as i64)),
                EvalValue::List(items) => Ok(EvalValue::Int(items.len() as i64)),
                EvalValue::Tuple(items) => Ok(EvalValue::Int(items.len() as i64)),
                EvalValue::Dict(entries) => Ok(EvalValue::Int(entries.len() as i64)),
                EvalValue::Null => Ok(EvalValue::Int(0)),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "len() unsupported target type".to_string(),
                )),
            }
        }
        "join" => {
            let sep = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("join() expects 1 argument".to_string())
            })?;
            let sep_str = eval_value_to_string(sep.clone());
            match target {
                EvalValue::List(values) | EvalValue::Tuple(values) => {
                    let parts: Vec<String> = values.into_iter().map(eval_value_to_string).collect();
                    Ok(EvalValue::Str(parts.join(&sep_str)))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "join() expects list/tuple target".to_string(),
                )),
            }
        }
        "split" => {
            let sep = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("split() expects 1 argument".to_string())
            })?;
            let sep_str = eval_value_to_string(sep.clone());
            let input_str = eval_value_to_string(target);
            let parts: Vec<String> = if sep_str.is_empty() {
                input_str.chars().map(|c| c.to_string()).collect()
            } else {
                input_str.split(&sep_str).map(|s| s.to_string()).collect()
            };
            Ok(EvalValue::List(
                parts.into_iter().map(EvalValue::Str).collect(),
            ))
        }
        "keys" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "keys() expects 0 arguments".to_string(),
                ));
            }
            match target {
                EvalValue::Dict(entries) => {
                    let keys = entries.into_iter().map(|(k, _)| k).collect();
                    Ok(EvalValue::List(keys))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "keys() expects dict target".to_string(),
                )),
            }
        }
        "values" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "values() expects 0 arguments".to_string(),
                ));
            }
            match target {
                EvalValue::Dict(entries) => {
                    let values = entries.into_iter().map(|(_, v)| v).collect();
                    Ok(EvalValue::List(values))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "values() expects dict target".to_string(),
                )),
            }
        }
        "get" => {
            let key = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("get() expects 1 or 2 arguments".to_string())
            })?;
            let default = args.get(1).cloned().unwrap_or(EvalValue::Null);
            let key = unwrap_optional(key.clone());
            match target {
                EvalValue::Dict(entries) => Ok(dict_get(entries, key).unwrap_or(default)),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "get() expects dict target".to_string(),
                )),
            }
        }
        "has" => {
            let key = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("has() expects 1 argument".to_string())
            })?;
            let key = unwrap_optional(key.clone());
            match target {
                EvalValue::Dict(entries) => Ok(EvalValue::Bool(dict_has(entries, key))),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "has() expects dict target".to_string(),
                )),
            }
        }
        "upper" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "upper() expects 0 arguments".to_string(),
                ));
            }
            Ok(EvalValue::Str(eval_value_to_string(target).to_uppercase()))
        }
        "lower" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "lower() expects 0 arguments".to_string(),
                ));
            }
            Ok(EvalValue::Str(eval_value_to_string(target).to_lowercase()))
        }
        "trim" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "trim() expects 0 arguments".to_string(),
                ));
            }
            Ok(EvalValue::Str(
                eval_value_to_string(target).trim().to_string(),
            ))
        }
        "replace" => {
            let old = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("replace() expects 2 arguments".to_string())
            })?;
            let new = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("replace() expects 2 arguments".to_string())
            })?;
            let input = eval_value_to_string(target);
            let old = eval_value_to_string(unwrap_optional(old.clone()));
            let new = eval_value_to_string(unwrap_optional(new.clone()));
            Ok(EvalValue::Str(input.replace(&old, &new)))
        }
        "contains" => {
            let needle = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("contains() expects 1 argument".to_string())
            })?;
            let input = eval_value_to_string(target);
            let needle = eval_value_to_string(unwrap_optional(needle.clone()));
            Ok(EvalValue::Bool(input.contains(&needle)))
        }
        "first" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "first() expects 0 arguments".to_string(),
                ));
            }
            match target {
                EvalValue::List(items) | EvalValue::Tuple(items) => {
                    Ok(items.into_iter().next().unwrap_or(EvalValue::Null))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "first() expects list/tuple target".to_string(),
                )),
            }
        }
        "last" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "last() expects 0 arguments".to_string(),
                ));
            }
            match target {
                EvalValue::List(items) | EvalValue::Tuple(items) => {
                    Ok(items.into_iter().last().unwrap_or(EvalValue::Null))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "last() expects list/tuple target".to_string(),
                )),
            }
        }
        "push" => {
            let item = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("push() expects 1 argument".to_string())
            })?;
            let item = unwrap_optional(item.clone());
            match target {
                EvalValue::List(mut items) => {
                    items.push(item);
                    Ok(EvalValue::List(items))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "push() expects list target".to_string(),
                )),
            }
        }
        "pop" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "pop() expects 0 arguments".to_string(),
                ));
            }
            match target {
                EvalValue::List(mut items) => {
                    let value = items.pop().unwrap_or(EvalValue::Null);
                    Ok(EvalValue::Tuple(vec![EvalValue::List(items), value]))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "pop() expects list target".to_string(),
                )),
            }
        }
        "insert" => {
            let index = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("insert() expects 2 arguments".to_string())
            })?;
            let value = args.get(1).ok_or_else(|| {
                ExecutionError::InvalidVarDecl("insert() expects 2 arguments".to_string())
            })?;
            let index = unwrap_optional(index.clone());
            let value = unwrap_optional(value.clone());
            match target {
                EvalValue::List(mut items) => {
                    let idx = eval_index_as_usize(index)?;
                    let idx = idx.min(items.len());
                    items.insert(idx, value);
                    Ok(EvalValue::List(items))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "insert() expects list target".to_string(),
                )),
            }
        }
        "remove" => {
            let index = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("remove() expects 1 argument".to_string())
            })?;
            let index = unwrap_optional(index.clone());
            match target {
                EvalValue::List(mut items) => {
                    let idx = eval_index_as_usize(index)?;
                    let value = if idx < items.len() {
                        items.remove(idx)
                    } else {
                        EvalValue::Null
                    };
                    Ok(EvalValue::Tuple(vec![EvalValue::List(items), value]))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "remove() expects list target".to_string(),
                )),
            }
        }
        "slice" => {
            let start = args.first().ok_or_else(|| {
                ExecutionError::InvalidVarDecl("slice() expects 1 or 2 arguments".to_string())
            })?;
            let end = args.get(1).cloned();
            let start = unwrap_optional(start.clone());
            let end = end.map(unwrap_optional);
            match target {
                EvalValue::List(items) => {
                    let s = eval_index_as_usize(start)?;
                    let e = match end {
                        Some(v) => eval_index_as_usize(v)?,
                        None => items.len(),
                    };
                    let s = s.min(items.len());
                    let e = e.min(items.len());
                    let out = if s >= e {
                        Vec::new()
                    } else {
                        items[s..e].to_vec()
                    };
                    Ok(EvalValue::List(out))
                }
                _ => Err(ExecutionError::InvalidVarDecl(
                    "slice() expects list target".to_string(),
                )),
            }
        }
        "__call__" => {
            if !args.is_empty() {
                return Err(ExecutionError::InvalidVarDecl(
                    "__call__ expects 0 arguments".to_string(),
                ));
            }
            match target {
                EvalValue::Str(name) => eval_func_call(&name, &[]),
                _ => Err(ExecutionError::InvalidVarDecl(
                    "__call__ expects string target".to_string(),
                )),
            }
        }
        _ => Err(ExecutionError::InvalidVarDecl(format!(
            "unknown method in var declaration: {}",
            name
        ))),
    }?;

    if target_optional {
        Ok(EvalValue::Optional(Box::new(result)))
    } else {
        Ok(result)
    }
}

pub(crate) fn is_truthy(value: &EvalValue) -> bool {
    match value {
        EvalValue::Null => false,
        EvalValue::Bool(b) => *b,
        EvalValue::Int(n) => *n != 0,
        EvalValue::Float(f) => *f != 0.0,
        EvalValue::Str(s) => !s.is_empty(),
        EvalValue::List(items) => !items.is_empty(),
        EvalValue::Dict(entries) => !entries.is_empty(),
        EvalValue::Tuple(items) => !items.is_empty(),
        EvalValue::Optional(inner) => is_truthy(inner),
    }
}

pub(crate) fn eval_value_equals(left: &EvalValue, right: &EvalValue) -> bool {
    let left = unwrap_optional(left.clone());
    let right = unwrap_optional(right.clone());

    match (left, right) {
        (EvalValue::Null, EvalValue::Null) => true,
        (EvalValue::Bool(a), EvalValue::Bool(b)) => a == b,
        (EvalValue::Int(a), EvalValue::Int(b)) => a == b,
        (EvalValue::Float(a), EvalValue::Float(b)) => a == b,
        (EvalValue::Int(a), EvalValue::Float(b)) => (a as f64) == b,
        (EvalValue::Float(a), EvalValue::Int(b)) => a == (b as f64),
        (EvalValue::Str(a), EvalValue::Str(b)) => a == b,
        (EvalValue::List(a), EvalValue::List(b)) | (EvalValue::Tuple(a), EvalValue::Tuple(b)) => {
            if a.len() != b.len() {
                return false;
            }
            for (left_item, right_item) in a.iter().zip(b.iter()) {
                if !eval_value_equals(left_item, right_item) {
                    return false;
                }
            }
            true
        }
        (EvalValue::Dict(a), EvalValue::Dict(b)) => {
            use std::collections::HashMap;
            let mut left_map: HashMap<String, EvalValue> = HashMap::new();
            let mut right_map: HashMap<String, EvalValue> = HashMap::new();

            for (k, v) in a {
                left_map.insert(eval_value_to_string(k), v);
            }
            for (k, v) in b {
                right_map.insert(eval_value_to_string(k), v);
            }

            if left_map.len() != right_map.len() {
                return false;
            }

            for (k, v) in left_map {
                match right_map.get(&k) {
                    Some(other) if eval_value_equals(&v, other) => {}
                    _ => return false,
                }
            }
            true
        }
        _ => false,
    }
}

fn is_nullish(value: &EvalValue) -> bool {
    matches!(value, EvalValue::Null)
        || matches!(value, EvalValue::Optional(inner) if is_nullish(inner))
}

fn dict_get(entries: Vec<(EvalValue, EvalValue)>, key: EvalValue) -> Option<EvalValue> {
    let key_str = eval_value_to_string(key);
    for (k, v) in entries {
        if eval_value_to_string(k) == key_str {
            return Some(v);
        }
    }
    None
}

fn dict_has(entries: Vec<(EvalValue, EvalValue)>, key: EvalValue) -> bool {
    dict_get(entries, key).is_some()
}

fn eval_index_as_usize(value: EvalValue) -> Result<usize> {
    match value {
        EvalValue::Int(n) if n >= 0 => Ok(n as usize),
        EvalValue::Str(s) => s.trim().parse::<usize>().map_err(|_| {
            ExecutionError::InvalidVarDecl("index must be non-negative integer".to_string())
        }),
        _ => Err(ExecutionError::InvalidVarDecl(
            "index must be non-negative integer".to_string(),
        )),
    }
}

fn cmp_numbers(op: &colony_parser::BinaryOp, left: f64, right: f64) -> bool {
    use colony_parser::BinaryOp;
    match op {
        BinaryOp::Eq => left == right,
        BinaryOp::Ne => left != right,
        BinaryOp::Lt => left < right,
        BinaryOp::Le => left <= right,
        BinaryOp::Gt => left > right,
        BinaryOp::Ge => left >= right,
        _ => false,
    }
}

fn cmp_strings(op: &colony_parser::BinaryOp, left: &str, right: &str) -> bool {
    use colony_parser::BinaryOp;
    match op {
        BinaryOp::Eq => left == right,
        BinaryOp::Ne => left != right,
        BinaryOp::Lt => left < right,
        BinaryOp::Le => left <= right,
        BinaryOp::Gt => left > right,
        BinaryOp::Ge => left >= right,
        _ => false,
    }
}

/// Main executor for shell commands
pub struct Executor {
    context: ExpansionContext,
    jobs: JobTable,
    /// Tuple space for reactive environment (Rules 42-47)
    tuple_space: TupleSpace,
    /// Session ID for scoping tuples (shell PID)
    session_id: String,
    /// Extended channels (stdcontext, stdllm, stdtool, stddiag)
    channels: ChannelManager,
    /// Command history
    history: History,
    /// SQLite-backed history store for structured metadata (colony-history)
    history_store: Option<Arc<colony_history::HistoryStore>>,
    /// Loaded special files state (F030)
    special_files: Option<special_files::SpecialFilesState>,
    /// Heartbeat scheduler (F080)
    heartbeat_scheduler: heartbeat::HeartbeatScheduler,
    /// Reflection observer for collecting usage data
    observer: Arc<dyn ReflectionObserver>,
    /// Embedded HTTP server state (serve builtin)
    serve_state: serve::ServeState,
    /// Debug/X-Ray controller for stepping and tracing
    debug_controller: Option<Arc<tokio::sync::Mutex<colony_debug::DebugController>>>,
}

impl Executor {
    pub fn new(context: ExpansionContext) -> Self {
        let session_id = std::process::id().to_string();
        let hb_scheduler = heartbeat::HeartbeatScheduler::new(&session_id);
        Self {
            context,
            jobs: JobTable::new(),
            tuple_space: TupleSpace::new(),
            session_id,
            channels: ChannelManager::new(),
            history: History::new(),
            history_store: None,
            special_files: None,
            heartbeat_scheduler: hb_scheduler,
            observer: Arc::new(colony_reflection::NoopObserver),
            serve_state: serve::ServeState::new(),
            debug_controller: None,
        }
    }

    pub fn with_history(context: ExpansionContext, history: History) -> Self {
        let session_id = std::process::id().to_string();
        let hb_scheduler = heartbeat::HeartbeatScheduler::new(&session_id);
        Self {
            context,
            jobs: JobTable::new(),
            tuple_space: TupleSpace::new(),
            session_id,
            channels: ChannelManager::new(),
            history,
            history_store: None,
            special_files: None,
            heartbeat_scheduler: hb_scheduler,
            observer: Arc::new(colony_reflection::NoopObserver),
            serve_state: serve::ServeState::new(),
            debug_controller: None,
        }
    }

    /// Set the SQLite-backed history store.
    pub fn set_history_store(&mut self, store: Arc<colony_history::HistoryStore>) {
        self.history_store = Some(store);
    }

    /// Get the history store, if available.
    pub fn history_store(&self) -> Option<&Arc<colony_history::HistoryStore>> {
        self.history_store.as_ref()
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }

    /// Get reference to channel manager
    pub fn channels(&self) -> &ChannelManager {
        &self.channels
    }

    /// Get mutable reference to channel manager
    pub fn channels_mut(&mut self) -> &mut ChannelManager {
        &mut self.channels
    }

    /// Set the reflection observer
    pub fn set_observer(&mut self, observer: Arc<dyn ReflectionObserver>) {
        self.observer = observer;
    }

    /// Get reference to the reflection observer
    pub fn observer(&self) -> &Arc<dyn ReflectionObserver> {
        &self.observer
    }

    /// Set the debug/X-Ray controller for stepping and tracing.
    pub fn set_debug_controller(
        &mut self,
        controller: Arc<tokio::sync::Mutex<colony_debug::DebugController>>,
    ) {
        self.debug_controller = Some(controller);
    }

    /// Get reference to the debug controller, if set.
    pub fn debug_controller(
        &self,
    ) -> Option<&Arc<tokio::sync::Mutex<colony_debug::DebugController>>> {
        self.debug_controller.as_ref()
    }

    /// Get reference to job table
    pub fn jobs(&self) -> &JobTable {
        &self.jobs
    }

    /// Get reference to tuple space
    pub fn tuple_space(&self) -> &TupleSpace {
        &self.tuple_space
    }

    /// Get session ID (shell PID) for scoping tuples
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get reference to serve state (embedded HTTP server)
    pub fn serve_state(&self) -> &serve::ServeState {
        &self.serve_state
    }

    /// Get reference to loaded special files state
    pub fn special_files(&self) -> Option<&special_files::SpecialFilesState> {
        self.special_files.as_ref()
    }

    /// Load special files from a directory with hierarchy walking (triggered on cd/pushd/popd).
    pub fn load_special_files(&mut self, dir: &std::path::Path) {
        use colony_tuple::{Tuple, Value};

        match special_files::SpecialFilesLoader::load_with_hierarchy(dir) {
            Ok(state) => {
                // Seed context channel with CONTEXT.md entries
                for entry in &state.context_entries {
                    if let Some(ctx) = self.channels.context() {
                        ctx.add_text(format!("[{}] {}", entry.section, entry.content));
                    }
                }

                // Emit session-scoped tuples for loaded agents
                for agent in &state.agents {
                    self.tuple_space.out(Tuple::from(vec![
                        Value::Str("agent_loaded".to_string()),
                        Value::Str(self.session_id.clone()),
                        Value::Str(agent.name.clone()),
                        Value::Str(dir.display().to_string()),
                    ]));
                }

                // Emit per-file-type tuples
                let loaded_files = special_files::SpecialFilesLoader::collect_loaded_files(dir);
                for loaded in loaded_files.values() {
                    self.tuple_space.out(Tuple::from(vec![
                        Value::Str("special_files_loaded".to_string()),
                        Value::Str(self.session_id.clone()),
                        Value::Str(loaded.file_type.to_string()),
                        Value::Str(loaded.path.display().to_string()),
                    ]));
                }

                // Set heartbeat scheduler tuple space for reactive coordination
                self.heartbeat_scheduler
                    .set_tuple_space(self.tuple_space.clone());

                self.special_files = Some(state);
            }
            Err(e) => {
                tracing::warn!("Failed to load special files from {}: {}", dir.display(), e);
            }
        }
    }

    /// Get a reference to the heartbeat scheduler.
    pub fn heartbeat_scheduler(&self) -> &heartbeat::HeartbeatScheduler {
        &self.heartbeat_scheduler
    }

    /// Execute the heartbeat builtin subcommands.
    fn execute_heartbeat_builtin(&self, args: &[String]) -> Result<ExitStatus> {
        let subcmd = args.first().map(String::as_str).unwrap_or("");

        match subcmd {
            "start" => {
                let cwd = self
                    .context
                    .get_var_or_env("PWD")
                    .unwrap_or_else(|| ".".to_string());

                match self
                    .heartbeat_scheduler
                    .discover(std::path::Path::new(&cwd))
                {
                    Ok(path) => {
                        eprintln!("heartbeat: discovered {}", path.display());
                    }
                    Err(e) => {
                        eprintln!("heartbeat: {e}");
                        return Ok(ExitStatus::new(1));
                    }
                }

                match self.heartbeat_scheduler.start() {
                    Ok(()) => {
                        let config = self.heartbeat_scheduler.config();
                        eprintln!(
                            "heartbeat: scheduler started (interval={}s, mode={})",
                            config.interval_secs, config.mode,
                        );
                        Ok(ExitStatus::new(0))
                    }
                    Err(e) => {
                        eprintln!("heartbeat: {e}");
                        Ok(ExitStatus::new(1))
                    }
                }
            }
            "stop" => match self.heartbeat_scheduler.stop() {
                Ok(()) => {
                    eprintln!("heartbeat: scheduler stopped");
                    Ok(ExitStatus::new(0))
                }
                Err(e) => {
                    eprintln!("heartbeat: {e}");
                    Ok(ExitStatus::new(1))
                }
            },
            "run" => match self.heartbeat_scheduler.run_once() {
                Ok(run) => {
                    println!("Run {}: {} ({}ms)", run.id, run.status, run.duration_ms);
                    for finding in &run.findings {
                        println!("  {}: {}", finding.severity, finding.message);
                    }
                    if run.findings.is_empty() && !run.raw_output.is_empty() {
                        println!("{}", run.raw_output);
                    }
                    Ok(ExitStatus::new(0))
                }
                Err(e) => {
                    eprintln!("heartbeat: {e}");
                    Ok(ExitStatus::new(1))
                }
            },
            "status" => {
                let status = self.heartbeat_scheduler.status();
                if status.running {
                    println!("Heartbeat: running");
                } else {
                    println!("Heartbeat: stopped");
                }
                if let Some(ref path) = status.checklist_path {
                    println!("Checklist: {}", path.display());
                }
                println!("Interval: {}s", status.interval_secs);
                println!("Mode: {}", status.mode);
                if let Some(ref last) = status.last_run {
                    println!("Last run: {} ({})", last.status, last.id);
                }
                Ok(ExitStatus::new(0))
            }
            "interval" => {
                let Some(dur_str) = args.get(1) else {
                    eprintln!("heartbeat interval: missing duration (e.g. 5m, 1h)");
                    return Ok(ExitStatus::new(1));
                };
                let Some(secs) = heartbeat::parse_duration(dur_str) else {
                    eprintln!("heartbeat interval: invalid duration '{dur_str}'");
                    return Ok(ExitStatus::new(1));
                };
                self.heartbeat_scheduler.set_interval(secs);
                eprintln!("heartbeat: interval set to {secs}s");
                Ok(ExitStatus::new(0))
            }
            "log" => {
                let Some(store) = self.heartbeat_scheduler.store() else {
                    eprintln!("heartbeat: no store available (run 'heartbeat start' first)");
                    return Ok(ExitStatus::new(1));
                };

                // If a run_id is provided, show that specific run
                if let Some(run_id) = args.get(1) {
                    match store.get_run(run_id) {
                        Ok(Some(run)) => {
                            println!("Run: {}", run.id);
                            println!("Time: {}", run.timestamp);
                            println!("Duration: {}ms", run.duration_ms);
                            println!("Status: {}", run.status);
                            println!("Path: {}", run.checklist_path);
                            println!();
                            for finding in &run.findings {
                                println!("{}: {}", finding.severity, finding.message);
                            }
                            if !run.raw_output.is_empty() {
                                println!();
                                println!("--- Raw Output ---");
                                println!("{}", run.raw_output);
                            }
                        }
                        Ok(None) => {
                            eprintln!("heartbeat log: run '{run_id}' not found");
                            return Ok(ExitStatus::new(1));
                        }
                        Err(e) => {
                            eprintln!("heartbeat log: {e}");
                            return Ok(ExitStatus::new(1));
                        }
                    }
                    return Ok(ExitStatus::new(0));
                }

                // Show recent runs
                match store.recent_runs(20) {
                    Ok(runs) => {
                        if runs.is_empty() {
                            println!("No heartbeat runs recorded.");
                        } else {
                            println!(
                                "{:<10} {:>8} {:>12} {:>8}",
                                "Time", "Duration", "Status", "Findings"
                            );
                            for run in &runs {
                                let warn_count = run
                                    .findings
                                    .iter()
                                    .filter(|f| f.severity == heartbeat::Severity::Warn)
                                    .count();
                                let alert_count = run
                                    .findings
                                    .iter()
                                    .filter(|f| f.severity == heartbeat::Severity::Alert)
                                    .count();
                                let id_short = if run.id.len() > 8 {
                                    &run.id[..8]
                                } else {
                                    &run.id
                                };
                                println!(
                                    "{:<10} {:>6}ms {:>12} {}w/{}a  {}",
                                    run.timestamp,
                                    run.duration_ms,
                                    run.status,
                                    warn_count,
                                    alert_count,
                                    id_short,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("heartbeat log: {e}");
                        return Ok(ExitStatus::new(1));
                    }
                }
                Ok(ExitStatus::new(0))
            }
            "render" => {
                let Some(store) = self.heartbeat_scheduler.store() else {
                    eprintln!("heartbeat: no store available (run 'heartbeat start' first)");
                    return Ok(ExitStatus::new(1));
                };

                let checklist_content = self
                    .heartbeat_scheduler
                    .checklist_path()
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .unwrap_or_default();

                let config = self.heartbeat_scheduler.config();
                let runs = store.recent_runs(20).unwrap_or_default();

                let html = heartbeat::dashboard::render_dashboard(
                    &checklist_content,
                    &runs,
                    config.interval_secs,
                    &config.mode.to_string(),
                );
                print!("{html}");
                Ok(ExitStatus::new(0))
            }
            "" => {
                eprintln!("heartbeat: missing subcommand");
                eprintln!("Usage: heartbeat start|stop|run|status|interval|log|render");
                Ok(ExitStatus::new(1))
            }
            other => {
                eprintln!("heartbeat: unknown subcommand '{other}'");
                eprintln!("Usage: heartbeat start|stop|run|status|interval|log|render");
                Ok(ExitStatus::new(1))
            }
        }
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

    pub async fn run_return_trap(&mut self) -> i32 {
        use colony_expand::TrapAction;
        use colony_expand::TrapSignal;
        use colony_parser::pest_api::parse;

        let return_trap = self.context.get_trap(&TrapSignal::Return).cloned();

        if let Some(TrapAction::Command(cmd)) = return_trap {
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

    pub async fn run_debug_trap(&mut self) -> i32 {
        use colony_expand::TrapAction;
        use colony_expand::TrapSignal;
        use colony_parser::pest_api::parse;

        if self.context.in_trap() {
            return 0;
        }

        let debug_trap = self.context.get_trap(&TrapSignal::Debug).cloned();

        if let Some(TrapAction::Command(cmd)) = debug_trap {
            self.context.enter_trap();
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
                self.context.exit_trap();
                return last;
            }
            self.context.exit_trap();
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
                    self.run_return_trap().await;
                    self.context.exit_function();
                    self.context.set_positional_params(saved_params);
                    return Err(e);
                }
            }
        }

        self.run_return_trap().await;
        self.context.exit_function();
        self.context.set_positional_params(saved_params);
        Ok(ExitStatus::new(last_exit))
    }

    pub async fn execute_proc(
        &mut self,
        proc_def: &colony_parser::ProcDef,
        args: Vec<String>,
    ) -> Result<ExitStatus> {
        let saved_params = self.context.get_all_positional_params().to_vec();
        self.context.set_positional_params(args.clone());
        self.context.enter_function();

        for (i, param) in proc_def.params.iter().enumerate() {
            let value = if let Some(arg) = args.get(i) {
                arg.clone()
            } else if let Some(ref default_expr) = param.default {
                let eval = self.eval_expression(default_expr)?;
                eval_value_to_string(eval)
            } else {
                String::new()
            };
            self.context
                .declare_local(&param.name, Some(colony_expand::Variable::String(value)));
        }

        let mut last_exit = 0;
        for stmt in &proc_def.body {
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
                    self.run_return_trap().await;
                    self.context.exit_function();
                    self.context.set_positional_params(saved_params);
                    return Err(e);
                }
            }
        }

        self.run_return_trap().await;
        self.context.exit_function();
        self.context.set_positional_params(saved_params);
        Ok(ExitStatus::new(last_exit))
    }

    /// Execute a simple command with reflection observation.
    ///
    /// Wraps the core logic in `execute_simple_command_inner`, timing it and
    /// recording command and error observations through the reflection observer.
    pub async fn execute_simple_command(&mut self, cmd: &SimpleCommand) -> Result<ExitStatus> {
        let start = std::time::Instant::now();
        let result = self.execute_simple_command_inner(cmd).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let cmd_name = cmd.name.as_literal().unwrap_or("<unknown>");
        let cmd_args: Vec<String> = cmd
            .args
            .iter()
            .filter_map(|w| w.as_literal().map(String::from))
            .collect();

        let exit_code = match &result {
            Ok(status) => status.code(),
            Err(_) => -1,
        };

        let input = if cmd_args.is_empty() {
            cmd_name.to_string()
        } else {
            format!("{} {}", cmd_name, cmd_args.join(" "))
        };

        self.observer
            .observe_command(colony_reflection::CommandData {
                input,
                exit_code,
                duration_ms,
                pipeline_length: 1,
            })
            .await;

        if let Err(ref e) = result {
            if !matches!(
                e,
                ExecutionError::Return(_)
                    | ExecutionError::Break
                    | ExecutionError::Continue
                    | ExecutionError::Exit(_)
                    | ExecutionError::Errexit(_)
            ) {
                self.observer
                    .observe_error(colony_reflection::ErrorData {
                        kind: error_kind_str(e),
                        message: e.to_string(),
                        recoverable: is_recoverable_error(e),
                    })
                    .await;
            }
        }

        result
    }

    /// Core logic for simple command execution.
    ///
    /// Checks for aliases, then builtins, then external commands.
    ///
    /// **F024 Phase 2:** Processes channel writes from child processes via tuple space.
    async fn execute_simple_command_inner(&mut self, cmd: &SimpleCommand) -> Result<ExitStatus> {
        use colony_expand::{Expander, Variable};
        use colony_parser::AssignmentValue;

        self.run_debug_trap().await;

        // Check if this is a command that treats assignments as arguments
        // (alias, declare, export, local, readonly, typeset)
        let assignments_are_args = {
            // First word of command name (before expansion)
            let first_part = cmd.name.as_literal().unwrap_or("");
            matches!(
                first_part,
                "alias" | "declare" | "export" | "local" | "readonly" | "typeset"
            )
        };

        // 0. Process assignments (arr=(a b c) or VAR=value)
        // BUT if this is an assignment-accepting builtin, skip processing and pass as args
        let mut expander = Expander::with_executor_and_expr(
            self.context.clone(),
            crate::execute_and_capture,
            |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
        );

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
                                colony_parser::Expression::Int(n, _) => {
                                    arg_str.push_str(&n.to_string())
                                }
                                colony_parser::Expression::Str(s, _) => arg_str.push_str(s),
                                _ => {}
                            }
                            arg_str.push(']');
                        }
                        colony_parser::ArraySubscript::All => arg_str.push_str("[@]"),
                        colony_parser::ArraySubscript::Star => arg_str.push_str("[*]"),
                    }
                }
                arg_str.push_str(if assignment.op == colony_parser::AssignOp::Append {
                    "+="
                } else {
                    "="
                });

                match &assignment.value {
                    AssignmentValue::Word(word) => {
                        if let Some(lit) = word.as_literal() {
                            arg_str.push_str(lit);
                        } else {
                            // Expand the word to get the value
                            if let Ok(fields) = expander.expand_word(word) {
                                arg_str.push_str(
                                    &fields
                                        .iter()
                                        .map(|f| f.text.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" "),
                                );
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
                        let entry_strs: Vec<String> = entries
                            .iter()
                            .map(|(k, v)| format!("[{}]={}", k, v))
                            .collect();
                        arg_str.push_str(&entry_strs.join(" "));
                        arg_str.push(')');
                    }
                }

                extra_args.push(colony_parser::Word::literal(&arg_str, assignment.span));
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
                                colony_parser::ArraySubscript::Index(expr) => match &**expr {
                                    colony_parser::Expression::Variable { name, .. } => {
                                        if let Some(Variable::AssocArray(_)) =
                                            self.context.get_variable(&assignment.name)
                                        {
                                            name.clone()
                                        } else {
                                            self.context
                                                .get_var_or_env(name)
                                                .unwrap_or_else(|| name.clone())
                                        }
                                    }
                                    colony_parser::Expression::Str(s, _) => {
                                        self.expand_subscript_string(s)
                                    }
                                    colony_parser::Expression::Int(n, _) => n.to_string(),
                                    _ => String::new(),
                                },
                                _ => String::new(),
                            };

                            if let Some(Variable::AssocArray(existing)) =
                                self.context.get_variable(&assignment.name)
                            {
                                let mut map = existing.clone();
                                map.insert(key, value);
                                self.context.set_variable(
                                    assignment.name.clone(),
                                    Variable::AssocArray(map),
                                );
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
                                self.context.set_variable(
                                    assignment.name.clone(),
                                    Variable::Array(new_arr),
                                );
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
                                    let existing = self
                                        .context
                                        .get_var_or_env(&assignment.name)
                                        .and_then(|s| s.parse::<i64>().ok())
                                        .unwrap_or(0);
                                    let addend = value.parse::<i64>().unwrap_or(0);
                                    let result = (existing + addend).to_string();
                                    if !self.context.set_var(assignment.name.clone(), result) {
                                        eprintln!(
                                            "colony: {}: readonly variable",
                                            &assignment.name
                                        );
                                        return Err(ExecutionError::Errexit(1));
                                    }
                                } else {
                                    let existing = self
                                        .context
                                        .get_var_or_env(&assignment.name)
                                        .unwrap_or_default();
                                    if !self
                                        .context
                                        .set_var(assignment.name.clone(), existing + &value)
                                    {
                                        eprintln!(
                                            "colony: {}: readonly variable",
                                            &assignment.name
                                        );
                                        return Err(ExecutionError::Errexit(1));
                                    }
                                }
                            } else if !self.context.set_var(assignment.name.clone(), value) {
                                eprintln!("colony: {}: readonly variable", &assignment.name);
                                return Err(ExecutionError::Errexit(1));
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
                                let mut existing = match self.context.get_variable(&assignment.name)
                                {
                                    Some(Variable::Array(arr)) => arr.clone(),
                                    Some(Variable::String(s)) => vec![s.clone()],
                                    Some(Variable::AssocArray(_)) => Vec::new(),
                                    Some(Variable::NameRef(_)) => Vec::new(),
                                    None => Vec::new(),
                                };
                                existing.extend(new_values);
                                self.context.set_variable(
                                    assignment.name.clone(),
                                    Variable::Array(existing),
                                );
                            } else {
                                self.context.set_variable(
                                    assignment.name.clone(),
                                    Variable::Array(new_values),
                                );
                            }
                        }
                        AssignmentValue::AssocArray(entries) => {
                            use std::collections::HashMap;
                            let mut map = HashMap::new();
                            for (key, val) in entries {
                                map.insert(key.clone(), val.clone());
                            }
                            self.context
                                .set_variable(assignment.name.clone(), Variable::AssocArray(map));
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
        let extra_args_from_name: Vec<String> =
            name_fields[1..].iter().map(|f| f.text.clone()).collect();

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
            let _redirect_guard =
                builtin_redirect::BuiltinRedirectGuard::apply(&cmd.redirects, &mut self.context)?;
            return self.execute_function(&func_def, args).await;
        }

        if let Some(proc_def) = self.context.get_proc(command_name).cloned() {
            let mut args = extra_args_from_name;
            for arg_word in &cmd.args {
                let fields = expander.expand_word(arg_word)?;
                for field in fields {
                    args.push(field.text);
                }
            }

            self.context.sync_variables_from(expander.context());
            xtrace::trace_simple_command(&self.context, command_name, &args);
            let _redirect_guard =
                builtin_redirect::BuiltinRedirectGuard::apply(&cmd.redirects, &mut self.context)?;
            return self.execute_proc(&proc_def, args).await;
        }

        // Alias expansion with cycle detection and re-parsing support
        let mut expanding_aliases = std::collections::HashSet::new();
        match alias::expand_alias(command_name, &self.context, &expanding_aliases) {
            alias::AliasExpansionResult::Reparsed(statements) => {
                let mut last_exit = 0;
                for (i, stmt) in statements.iter().enumerate() {
                    if i == statements.len() - 1 {
                        if let colony_parser::Statement::SimpleCommand(mut last_cmd) = stmt.clone()
                        {
                            last_cmd.args.extend(cmd.args.clone());
                            last_cmd.redirects.extend(cmd.redirects.clone());
                            last_exit = self
                                .execute_statement_recursive(
                                    &colony_parser::Statement::SimpleCommand(last_cmd),
                                )
                                .await?;
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
            alias::AliasExpansionResult::SimpleExpansion {
                new_command,
                alias_args,
                expand_next,
            } => {
                expanding_aliases.insert(command_name.to_string());

                let mut new_cmd = cmd.clone();
                new_cmd.name = colony_parser::Word::literal(&new_command, cmd.span);

                let mut new_args = Vec::new();
                for alias_arg in &alias_args {
                    new_args.push(colony_parser::Word::literal(alias_arg, cmd.span));
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
                                } =
                                    alias::expand_alias(arg_name, &self.context, &expanding_aliases)
                                {
                                    new_args.push(colony_parser::Word::literal(
                                        &arg_cmd,
                                        first_arg.span,
                                    ));
                                    for arg in &arg_alias_args {
                                        new_args.push(colony_parser::Word::literal(
                                            arg,
                                            first_arg.span,
                                        ));
                                    }
                                    skip_first_arg = true;
                                }
                            }
                        }
                    }
                }

                let args_to_add = if skip_first_arg {
                    &cmd.args[1..]
                } else {
                    &cmd.args[..]
                };
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
                    let status = self
                        .execute_external_command_with_channels(&new_cmd)
                        .await?;
                    self.sync_channels_from_tuple_space();
                    Ok(status)
                })
                .await;
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

            let is_assignment_only_noop =
                command_name == ":" && cmd.args.is_empty() && !cmd.env_assignments.is_empty();
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
                Some(builtin_redirect::BuiltinRedirectGuard::apply(
                    &cmd.redirects,
                    &mut self.context,
                )?)
            };

            // Handle serve builtin with executor-owned state.
            // The "serve" command (not namespaced serve:status/serve:header)
            // needs access to ServeState on the Executor.
            if command_name == "serve" {
                let mut serve_args = extra_args_from_name.clone();
                for arg_word in &cmd.args {
                    if let Ok(fields) = expander.expand_word(arg_word) {
                        for field in fields {
                            serve_args.push(field.text);
                        }
                    }
                }
                return match serve::builtin_serve(
                    &serve_args,
                    &self.context,
                    &self.tuple_space,
                    &self.session_id,
                    &self.serve_state,
                ) {
                    Ok(result) => Ok(ExitStatus::new(result.exit_code())),
                    Err(e) => Err(e),
                };
            }

            // Handle heartbeat builtin with executor-owned state.
            if command_name == "heartbeat" {
                let mut hb_args = extra_args_from_name.clone();
                for arg_word in &cmd.args {
                    if let Ok(fields) = expander.expand_word(arg_word) {
                        for field in fields {
                            hb_args.push(field.text);
                        }
                    }
                }
                return self.execute_heartbeat_builtin(&hb_args);
            }

            if let Some(builtin_result) = execute_builtin(
                cmd,
                &mut self.context,
                &self.jobs,
                &self.tuple_space,
                &self.session_id,
                &mut self.channels,
                Some(&mut self.history),
                self.history_store.as_ref(),
            ) {
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
    async fn execute_external_command_with_channels(
        &mut self,
        cmd: &SimpleCommand,
    ) -> Result<ExitStatus> {
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
            let mut expander = Expander::with_executor_and_expr(
                context_clone.clone(),
                crate::execute_and_capture,
                |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
            );

            // Expand command name
            let name_fields = expander.expand_word(&cmd_clone.name)?;
            if name_fields.is_empty() {
                return Err(ExecutionError::CommandNotFound("<empty>".to_string()));
            }
            let command_name = &name_fields[0].text;
            let extra_args_from_name: Vec<String> =
                name_fields[1..].iter().map(|f| f.text.clone()).collect();

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

            let mut env = build_env(&context_clone);

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
            let mut handle = spawn_process(
                &program_path,
                &args,
                &env,
                &cmd_clone.redirects,
                &mut context_clone,
            )?;

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
        .map_err(|e| ExecutionError::IoError(std::io::Error::other(e)))?
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

    /// Execute an agent invocation: @agent_name "prompt" or @agent_name:tool "args"
    pub async fn execute_agent_invocation(
        &mut self,
        invocation: &colony_parser::AgentInvocation,
    ) -> Result<ExitStatus> {
        use colony_expand::Expander;

        let mut expander = Expander::with_executor_and_expr(
            self.context.clone(),
            crate::execute_and_capture,
            |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
        );

        let prompt_fields = expander.expand_word(&invocation.prompt).map_err(|_| {
            ExecutionError::CommandNotFound("agent prompt expansion failed".to_string())
        })?;
        let prompt_text: String = prompt_fields
            .iter()
            .map(|f| f.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let agent_name = &invocation.agent_name;
        let tool = invocation.tool.as_deref();

        match tool {
            Some(tool_name) => {
                println!("@{}:{} {}", agent_name, tool_name, prompt_text);
            }
            None => {
                println!("@{} {}", agent_name, prompt_text);
            }
        }

        self.context
            .set_var("AGENT_NAME".to_string(), agent_name.clone());
        self.context
            .set_var("AGENT_PROMPT".to_string(), prompt_text);
        if let Some(t) = tool {
            self.context
                .set_var("AGENT_TOOL".to_string(), t.to_string());
        }

        Ok(ExitStatus::success())
    }

    pub async fn execute_source_file(
        &mut self,
        file_path: &std::path::Path,
        args: Vec<String>,
    ) -> Result<ExitStatus> {
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
            self.context
                .get_var_or_env(var_name)
                .unwrap_or_else(|| s.to_string())
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

            // Debug/X-Ray hook: notify controller before dispatch
            let debug_start = if self.debug_controller.is_some() {
                Some(std::time::Instant::now())
            } else {
                None
            };
            if let Some(ref debug) = self.debug_controller {
                let line = stmt.span().start.line;
                let source = format!("{:?}", std::mem::discriminant(stmt));
                let mut ctrl = debug.lock().await;
                let should_proceed = ctrl.on_statement_enter(line, &source);
                if !should_proceed {
                    ctrl.wait_for_command().await;
                }
            }

            let result = match stmt {
                Statement::SimpleCommand(cmd) => {
                    let status = Box::pin(self.execute_simple_command(cmd)).await?;
                    let code = status.code();
                    self.context.set_last_exit_status(code);
                    self.check_errexit(code)
                }
                Statement::Pipeline(pipeline) => {
                    if pipeline.elements.len() == 1 && !pipeline.elements[0].negated {
                        return self
                            .execute_statement_recursive(&pipeline.elements[0].command)
                            .await;
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
                            _ => 0,
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
                Statement::For(for_stmt) => self.execute_for_loop_internal(for_stmt).await,
                Statement::If(if_stmt) => self.execute_if_internal(if_stmt).await,
                Statement::Try(try_stmt) => self.execute_try_internal(try_stmt).await,
                Statement::Match(match_stmt) => self.execute_match_internal(match_stmt).await,
                Statement::While(while_stmt) => self.execute_while_internal(while_stmt).await,
                Statement::Until(until_stmt) => self.execute_until_internal(until_stmt).await,
                Statement::Case(case_stmt) => self.execute_case_internal(case_stmt).await,
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
                    self.context
                        .define_function(func_def.name.clone(), func_def.clone());
                    Ok(0)
                }
                Statement::ProcDef(proc_def) => {
                    self.context
                        .define_proc(proc_def.name.clone(), proc_def.clone());
                    Ok(0)
                }
                Statement::VarDecl(decl) => {
                    let attrs = self.var_decl_attributes(decl);

                    match &decl.target {
                        colony_parser::VarDeclTarget::Name(name) => {
                            let value = self.evaluate_var_decl_value(&decl.value)?;
                            self.apply_var_decl(name, value, attrs, decl.kind)?;
                            Ok(0)
                        }
                        colony_parser::VarDeclTarget::Tuple(names) => {
                            let eval_value = self.eval_expression(&decl.value)?;
                            let values = match eval_value {
                                EvalValue::List(items) | EvalValue::Tuple(items) => items,
                                EvalValue::Dict(_) => {
                                    return Err(ExecutionError::InvalidVarDecl(
                                        "cannot destructure dict into tuple".to_string(),
                                    ))
                                }
                                EvalValue::Null => {
                                    return Err(ExecutionError::InvalidVarDecl(
                                        "tuple destructure requires tuple/list, got null"
                                            .to_string(),
                                    ))
                                }
                                other => {
                                    return Err(ExecutionError::InvalidVarDecl(format!(
                                        "tuple destructure requires tuple/list, got {}",
                                        eval_value_to_string(other)
                                    )))
                                }
                            };

                            if names.len() != values.len() {
                                return Err(ExecutionError::InvalidVarDecl(format!(
                                    "tuple destructure arity mismatch: {} names, {} values",
                                    names.len(),
                                    values.len()
                                )));
                            }

                            for (name, value) in names.iter().zip(values) {
                                let var = eval_value_to_variable(value)?;
                                self.apply_var_decl(name, var, attrs, decl.kind)?;
                            }
                            Ok(0)
                        }
                    }
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
                Statement::Conditional(cond_cmd) => {
                    let result = self.evaluate_conditional_expr(&cond_cmd.expr);
                    Ok(if result { 0 } else { 1 })
                }
                other => {
                    eprintln!("Statement type not yet supported in function: {:?}", other);
                    Ok(1)
                }
            };

            // Debug/X-Ray hook: notify controller after dispatch
            if let (Some(start), Some(ref debug)) = (debug_start, &self.debug_controller) {
                let elapsed = start.elapsed();
                let mut ctrl = debug.lock().await;
                ctrl.on_statement_exit(stmt.span().start.line, elapsed.as_micros() as u64);
            }

            result
        })
    }

    pub async fn execute_try_internal(
        &mut self,
        try_stmt: &colony_parser::TryStatement,
    ) -> Result<i32> {
        let mut last_exit = self.context.last_exit_status();
        let mut pending_error: Option<ExecutionError> = None;
        let mut error_line: Option<u32> = None;

        for stmt in &try_stmt.body {
            match self.execute_statement_recursive(stmt).await {
                Ok(code) => {
                    last_exit = code;
                    self.context.set_last_exit_status(code);
                }
                Err(e) => {
                    error_line = Some(stmt.span().start.line);
                    pending_error = Some(e);
                    break;
                }
            }
        }

        if let Some(err) = pending_error.take() {
            if matches!(
                err,
                ExecutionError::Return(_)
                    | ExecutionError::Break
                    | ExecutionError::Continue
                    | ExecutionError::Exit(_)
            ) {
                pending_error = Some(err);
            } else {
                self.set_error_vars(&err, error_line);

                if let Some(catch_block) = &try_stmt.catch {
                    if self.catch_matches(&err, catch_block) {
                        if let Some(binding) = &catch_block.binding {
                            let _ = self.context.set_var(binding.clone(), err.to_string());
                        }

                        let mut catch_exit = 0;
                        for stmt in &catch_block.body {
                            match self.execute_statement_recursive(stmt).await {
                                Ok(code) => {
                                    catch_exit = code;
                                    self.context.set_last_exit_status(code);
                                }
                                Err(e) => {
                                    pending_error = Some(e);
                                    break;
                                }
                            }
                        }

                        if pending_error.is_none() {
                            last_exit = catch_exit;
                        }
                    } else {
                        pending_error = Some(err);
                    }
                } else {
                    pending_error = Some(err);
                }
            }
        }

        if let Some(finally_body) = &try_stmt.finally_body {
            let mut finally_exit = last_exit;
            for stmt in finally_body {
                match self.execute_statement_recursive(stmt).await {
                    Ok(code) => {
                        finally_exit = code;
                        self.context.set_last_exit_status(code);
                    }
                    Err(e) => return Err(e),
                }
            }
            last_exit = finally_exit;
        }

        if let Some(err) = pending_error {
            return Err(err);
        }

        Ok(last_exit)
    }

    fn catch_matches(&self, err: &ExecutionError, catch_block: &colony_parser::CatchBlock) -> bool {
        match &catch_block.error_type {
            Some(expected) => expected == error_type_name(err),
            None => true,
        }
    }

    fn set_error_vars(&mut self, err: &ExecutionError, line: Option<u32>) {
        let _ = self.context.set_var("ERROR".to_string(), err.to_string());
        let _ = self
            .context
            .set_var("ERROR_CODE".to_string(), error_code(err).to_string());
        let _ = self
            .context
            .set_var("ERROR_COMMAND".to_string(), String::new());
        let _ = self
            .context
            .set_var("ERROR_LINE".to_string(), line.unwrap_or(0).to_string());
        let _ = self
            .context
            .set_var("ERROR_TYPE".to_string(), error_type_name(err).to_string());
    }

    fn var_decl_attributes(&self, decl: &colony_parser::VarDecl) -> VarAttributes {
        let mut attrs = VarAttributes::default();

        if matches!(decl.kind, colony_parser::VarKind::Const) {
            attrs.readonly = true;
        }

        if let Some(type_name) = decl.type_annotation.as_deref() {
            let mut base = String::new();
            for ch in type_name.chars() {
                if ch.is_alphanumeric() || ch == '_' {
                    base.push(ch);
                } else {
                    break;
                }
            }

            if base == "Int" {
                attrs.integer = true;
            }
        }

        attrs
    }

    fn evaluate_var_decl_value(&mut self, expr: &colony_parser::Expression) -> Result<Variable> {
        let value = self.eval_expression(expr)?;
        eval_value_to_variable(value)
    }

    fn eval_expression(&mut self, expr: &colony_parser::Expression) -> Result<EvalValue> {
        eval_expression_in_context(&mut self.context, expr)
    }

    fn apply_var_decl(
        &mut self,
        name: &str,
        value: Variable,
        attrs: VarAttributes,
        kind: colony_parser::VarKind,
    ) -> Result<()> {
        if attrs.integer && !matches!(value, Variable::String(_)) {
            return Err(ExecutionError::InvalidVarDecl(format!(
                "{}: Int requires scalar value",
                name
            )));
        }

        match kind {
            colony_parser::VarKind::Var | colony_parser::VarKind::Const => {
                if self.context.in_function() {
                    self.context
                        .declare_local_with_attrs(name, Some(value), Some(attrs));
                } else if let Variable::String(s) = value {
                    self.context
                        .declare_with_attrs(name, Some(s), attrs)
                        .map_err(|e| ExecutionError::InvalidVarDecl(format!("{}: {}", name, e)))?;
                } else {
                    self.context.set_var_attributes(name, attrs);
                    self.context.set_variable(name.to_string(), value);
                }
                Ok(())
            }
            colony_parser::VarKind::Setvar => {
                if self.context.get_variable(name).is_none() {
                    return Err(ExecutionError::InvalidVarDecl(format!(
                        "{}: not declared",
                        name
                    )));
                }
                if self.context.is_readonly(name) {
                    return Err(ExecutionError::InvalidVarDecl(format!(
                        "{}: readonly variable",
                        name
                    )));
                }
                if let Variable::String(s) = value {
                    self.context
                        .set_var_with_attrs(name.to_string(), s, Some(attrs))
                        .map_err(|e| ExecutionError::InvalidVarDecl(format!("{}: {}", name, e)))?;
                } else {
                    self.context.set_var_attributes(name, attrs);
                    self.context.set_variable(name.to_string(), value);
                }
                Ok(())
            }
        }
    }

    async fn execute_for_loop_internal(
        &mut self,
        for_stmt: &colony_parser::ForStatement,
    ) -> Result<i32> {
        use colony_expand::Expander;

        let _redirect_guard =
            builtin_redirect::BuiltinRedirectGuard::apply(&for_stmt.redirects, &mut self.context)?;

        let mut expander = Expander::with_executor_and_expr(
            self.context.clone(),
            crate::execute_and_capture,
            |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
        );

        let mut words_expanded = Vec::new();
        for word in &for_stmt.words {
            let fields = expander.expand_word(word)?;
            for field in fields {
                words_expanded.push(field.text);
            }
        }

        let mut last_exit = 0;
        for value in words_expanded {
            if self.context.is_readonly(&for_stmt.var) {
                eprintln!("colony: {}: readonly variable", for_stmt.var);
                return Ok(1);
            }
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

    async fn execute_condition_internal(
        &mut self,
        condition: &colony_parser::Condition,
    ) -> Result<i32> {
        let saved_errexit = self.context.shell_options().errexit;
        self.context.shell_options_mut().errexit = false;

        let result = match condition {
            colony_parser::Condition::Command(cmd) => self.execute_statement_recursive(cmd).await,
            colony_parser::Condition::Expression(_) => Ok(1),
        };

        self.context.shell_options_mut().errexit = saved_errexit;
        result
    }

    async fn execute_while_internal(
        &mut self,
        while_stmt: &colony_parser::WhileStatement,
    ) -> Result<i32> {
        let _redirect_guard = builtin_redirect::BuiltinRedirectGuard::apply(
            &while_stmt.redirects,
            &mut self.context,
        )?;

        let mut last_exit = 0;
        loop {
            let cond_exit = self
                .execute_condition_internal(&while_stmt.condition)
                .await?;
            if cond_exit != 0 {
                break;
            }
            for stmt in &while_stmt.body {
                last_exit = self.execute_statement_recursive(stmt).await?;
            }
        }
        Ok(last_exit)
    }

    async fn execute_until_internal(
        &mut self,
        until_stmt: &colony_parser::UntilStatement,
    ) -> Result<i32> {
        let _redirect_guard = builtin_redirect::BuiltinRedirectGuard::apply(
            &until_stmt.redirects,
            &mut self.context,
        )?;

        let mut last_exit = 0;
        loop {
            let cond_exit = self
                .execute_condition_internal(&until_stmt.condition)
                .await?;
            if cond_exit == 0 {
                break;
            }
            for stmt in &until_stmt.body {
                last_exit = self.execute_statement_recursive(stmt).await?;
            }
        }
        Ok(last_exit)
    }

    pub async fn execute_match_internal(
        &mut self,
        match_stmt: &colony_parser::MatchStatement,
    ) -> Result<i32> {
        let value = self.eval_expression(&match_stmt.value)?;

        for arm in &match_stmt.arms {
            let mut bindings: Vec<(String, EvalValue)> = Vec::new();
            if !self.match_pattern_internal(&value, &arm.pattern, &mut bindings)? {
                continue;
            }

            let saved = self.apply_match_bindings(&bindings)?;
            let guard_ok = if let Some(guard) = &arm.guard {
                let guard_val = self.eval_expression(guard)?;
                is_truthy(&guard_val)
            } else {
                true
            };

            if !guard_ok {
                self.restore_match_bindings(saved);
                continue;
            }

            let code = match &arm.body {
                colony_parser::MatchBody::Block(stmts) => {
                    let mut last = 0;
                    for stmt in stmts {
                        last = self.execute_statement_recursive(stmt).await?;
                        self.context.set_last_exit_status(last);
                    }
                    last
                }
                colony_parser::MatchBody::Expr(expr) => {
                    let value = self.eval_expression(expr)?;
                    if is_truthy(&value) {
                        0
                    } else {
                        1
                    }
                }
            };

            self.context.set_last_exit_status(code);
            return Ok(code);
        }

        self.context.set_last_exit_status(0);
        Ok(0)
    }

    async fn execute_case_internal(
        &mut self,
        case_stmt: &colony_parser::CaseStatement,
    ) -> Result<i32> {
        use colony_expand::Expander;

        let mut expander = Expander::with_executor_and_expr(
            self.context.clone(),
            crate::execute_and_capture,
            |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
        );

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
                    colony_parser::CaseTerminator::Continue => {}
                }
            }
        }

        Ok(last_exit)
    }

    fn match_pattern_internal(
        &mut self,
        value: &EvalValue,
        pattern: &colony_parser::MatchPattern,
        bindings: &mut Vec<(String, EvalValue)>,
    ) -> Result<bool> {
        use colony_parser::MatchPattern;

        let value = unwrap_optional(value.clone());

        match pattern {
            MatchPattern::Wildcard => Ok(true),
            MatchPattern::Binding(name) => Ok(self.bind_match_value(bindings, name, &value)),
            MatchPattern::Literal(expr) => {
                let expected = self.eval_expression(expr)?;
                Ok(eval_value_equals(&value, &expected))
            }
            MatchPattern::List(list) => match value {
                EvalValue::List(items) => self.match_list_pattern(items, list, bindings),
                EvalValue::Tuple(items) => self.match_list_pattern(items, list, bindings),
                _ => Ok(false),
            },
            MatchPattern::Dict(dict) => match value {
                EvalValue::Dict(entries) => self.match_dict_pattern(entries, dict, bindings),
                _ => Ok(false),
            },
            MatchPattern::Tuple(tuple) => match value {
                EvalValue::Tuple(items) => self.match_tuple_pattern(items, tuple, bindings),
                _ => Ok(false),
            },
            MatchPattern::Or(patterns) => {
                for pat in patterns {
                    let mut trial_bindings = Vec::new();
                    if self.match_pattern_internal(&value, pat, &mut trial_bindings)? {
                        bindings.extend(trial_bindings);
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    fn match_list_pattern(
        &mut self,
        items: Vec<EvalValue>,
        list: &colony_parser::ListPattern,
        bindings: &mut Vec<(String, EvalValue)>,
    ) -> Result<bool> {
        if items.len() < list.items.len() {
            return Ok(false);
        }

        for (item, pattern) in items.iter().zip(list.items.iter()) {
            if !self.match_pattern_internal(item, pattern, bindings)? {
                return Ok(false);
            }
        }

        match list.rest {
            colony_parser::ListRest::None => Ok(items.len() == list.items.len()),
            colony_parser::ListRest::Ignore => Ok(true),
            colony_parser::ListRest::Bind(ref name) => {
                let rest = items[list.items.len()..].to_vec();
                Ok(self.bind_match_value(bindings, name, &EvalValue::List(rest)))
            }
        }
    }

    fn match_dict_pattern(
        &mut self,
        entries: Vec<(EvalValue, EvalValue)>,
        dict: &colony_parser::DictPattern,
        bindings: &mut Vec<(String, EvalValue)>,
    ) -> Result<bool> {
        use std::collections::{HashMap, HashSet};

        let mut map: HashMap<String, EvalValue> = HashMap::new();
        for (k, v) in entries {
            map.insert(eval_value_to_string(k), v);
        }

        let mut seen: HashSet<String> = HashSet::new();

        for field in &dict.fields {
            let key = field.name.clone();
            let value = match map.get(&key) {
                Some(val) => val.clone(),
                None => return Ok(false),
            };
            seen.insert(key.clone());

            if let Some(pattern) = &field.pattern {
                if !self.match_pattern_internal(&value, pattern, bindings)? {
                    return Ok(false);
                }
            } else if !self.bind_match_value(bindings, &key, &value) {
                return Ok(false);
            }
        }

        if !dict.allow_extra {
            for key in map.keys() {
                if !seen.contains(key) {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    fn match_tuple_pattern(
        &mut self,
        items: Vec<EvalValue>,
        tuple: &colony_parser::TuplePattern,
        bindings: &mut Vec<(String, EvalValue)>,
    ) -> Result<bool> {
        if items.len() != tuple.items.len() {
            return Ok(false);
        }

        for (item, pattern) in items.iter().zip(tuple.items.iter()) {
            if !self.match_pattern_internal(item, pattern, bindings)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn bind_match_value(
        &self,
        bindings: &mut Vec<(String, EvalValue)>,
        name: &str,
        value: &EvalValue,
    ) -> bool {
        for (existing_name, existing_value) in bindings.iter() {
            if existing_name == name {
                return eval_value_equals(existing_value, value);
            }
        }

        bindings.push((name.to_string(), value.clone()));
        true
    }

    fn apply_match_bindings(
        &mut self,
        bindings: &[(String, EvalValue)],
    ) -> Result<Vec<(String, Option<Variable>)>> {
        let mut saved = Vec::new();

        for (name, value) in bindings {
            let previous = self.context.get_variable(name).cloned();
            saved.push((name.clone(), previous));

            if self.context.is_readonly(name) {
                return Err(ExecutionError::InvalidVarDecl(format!(
                    "readonly variable: {}",
                    name
                )));
            }

            let var = eval_value_to_variable(value.clone())?;
            self.context.set_variable(name.clone(), var);
        }

        Ok(saved)
    }

    fn restore_match_bindings(&mut self, saved: Vec<(String, Option<Variable>)>) {
        for (name, previous) in saved {
            match previous {
                Some(var) => self.context.set_variable(name, var),
                None => self.context.unset_variable(&name),
            }
        }
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

    fn evaluate_conditional_expr(&mut self, expr: &colony_parser::CondExpr) -> bool {
        use colony_parser::{CondBinaryOp, CondExpr, CondUnaryOp};

        match expr {
            CondExpr::And(left, right) => {
                self.evaluate_conditional_expr(left) && self.evaluate_conditional_expr(right)
            }
            CondExpr::Or(left, right) => {
                self.evaluate_conditional_expr(left) || self.evaluate_conditional_expr(right)
            }
            CondExpr::Not(inner) => !self.evaluate_conditional_expr(inner),
            CondExpr::Binary {
                left,
                op,
                right,
                right_quoted,
            } => {
                let left_expanded = self.expand_cond_operand(left);
                let right_expanded = self.expand_cond_operand(right);

                match op {
                    CondBinaryOp::Eq => {
                        if *right_quoted {
                            left_expanded == right_expanded
                        } else {
                            Self::glob_match_cond(&right_expanded, &left_expanded)
                        }
                    }
                    CondBinaryOp::Ne => {
                        if *right_quoted {
                            left_expanded != right_expanded
                        } else {
                            !Self::glob_match_cond(&right_expanded, &left_expanded)
                        }
                    }
                    CondBinaryOp::Match => match regex::Regex::new(&right_expanded) {
                        Ok(re) => {
                            if let Some(caps) = re.captures(&left_expanded) {
                                let mut bash_rematch = Vec::new();
                                for i in 0..caps.len() {
                                    bash_rematch.push(
                                        caps.get(i).map(|m| m.as_str()).unwrap_or("").to_string(),
                                    );
                                }
                                self.context.set_variable(
                                    "BASH_REMATCH".to_string(),
                                    colony_expand::Variable::Array(bash_rematch),
                                );
                                true
                            } else {
                                self.context.set_variable(
                                    "BASH_REMATCH".to_string(),
                                    colony_expand::Variable::Array(vec![]),
                                );
                                false
                            }
                        }
                        Err(_) => false,
                    },
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
                        use std::fs;
                        use std::os::unix::fs::MetadataExt;
                        match (fs::metadata(&left_expanded), fs::metadata(&right_expanded)) {
                            (Ok(m1), Ok(m2)) => m1.dev() == m2.dev() && m1.ino() == m2.ino(),
                            _ => false,
                        }
                    }
                }
            }
            CondExpr::Unary { op, arg } => {
                let arg_expanded = self.expand_cond_operand(arg);
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
                        path.metadata()
                            .map(|m| m.permissions().mode() & 0o111 != 0)
                            .unwrap_or(false)
                    }
                    CondUnaryOp::NonEmpty => path.metadata().map(|m| m.len() > 0).unwrap_or(false),
                    CondUnaryOp::IsSymlink => path.is_symlink(),
                    CondUnaryOp::IsPipe => {
                        use std::os::unix::fs::FileTypeExt;
                        path.metadata()
                            .map(|m| m.file_type().is_fifo())
                            .unwrap_or(false)
                    }
                    CondUnaryOp::IsSocket => {
                        use std::os::unix::fs::FileTypeExt;
                        path.metadata()
                            .map(|m| m.file_type().is_socket())
                            .unwrap_or(false)
                    }
                    CondUnaryOp::IsBlock => {
                        use std::os::unix::fs::FileTypeExt;
                        path.metadata()
                            .map(|m| m.file_type().is_block_device())
                            .unwrap_or(false)
                    }
                    CondUnaryOp::IsChar => {
                        use std::os::unix::fs::FileTypeExt;
                        path.metadata()
                            .map(|m| m.file_type().is_char_device())
                            .unwrap_or(false)
                    }
                    CondUnaryOp::IsTerminal => arg_expanded
                        .parse::<i32>()
                        .map(|fd| unsafe { libc::isatty(fd) } != 0)
                        .unwrap_or(false),
                    CondUnaryOp::OwnedByEuid => {
                        use std::os::unix::fs::MetadataExt;
                        path.metadata()
                            .map(|m| m.uid() == unsafe { libc::geteuid() })
                            .unwrap_or(false)
                    }
                    CondUnaryOp::OwnedByEgid => {
                        use std::os::unix::fs::MetadataExt;
                        path.metadata()
                            .map(|m| m.gid() == unsafe { libc::getegid() })
                            .unwrap_or(false)
                    }
                    CondUnaryOp::ModifiedSinceRead => {
                        use std::os::unix::fs::MetadataExt;
                        path.metadata()
                            .map(|m| m.mtime() > m.atime())
                            .unwrap_or(false)
                    }
                    CondUnaryOp::VarIsSet => self.context.get_var_or_env(&arg_expanded).is_some(),
                }
            }
            CondExpr::String(s) => {
                let expanded = self.expand_cond_operand(s);
                !expanded.is_empty()
            }
        }
    }

    fn expand_cond_operand(&self, s: &str) -> String {
        if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
            s[1..s.len() - 1].to_string()
        } else if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            self.expand_double_quoted_cond(&s[1..s.len() - 1])
        } else if s.starts_with('$') {
            self.expand_variable_ref_cond(s)
        } else {
            s.to_string()
        }
    }

    fn expand_double_quoted_cond(&self, content: &str) -> String {
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
                            if ch == '(' {
                                depth += 1;
                            }
                            if ch == ')' {
                                depth -= 1;
                            }
                        } else {
                            break;
                        }
                    }
                    result.push_str(&var_str);
                    continue;
                } else {
                    while let Some(&ch) = chars.peek() {
                        if ch.is_alphanumeric() || ch == '_' || ch == '[' || ch == ']' {
                            var_str.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                }
                result.push_str(&self.expand_variable_ref_cond(&var_str));
            } else if c == '\\' {
                if let Some(&next) = chars.peek() {
                    if next == '$' || next == '\\' || next == '"' {
                        result.push(chars.next().unwrap());
                    } else {
                        result.push(c);
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    fn expand_variable_ref_cond(&self, s: &str) -> String {
        if s.starts_with("${") && s.ends_with('}') {
            let inner = &s[2..s.len() - 1];
            if let Some(bracket_pos) = inner.find('[') {
                let var_name = &inner[..bracket_pos];
                let rest = &inner[bracket_pos + 1..];
                if let Some(end) = rest.find(']') {
                    let index = &rest[..end];
                    if let Some(colony_expand::Variable::Array(arr)) =
                        self.context.get_variable(var_name)
                    {
                        if let Ok(idx) = index.parse::<usize>() {
                            return arr.get(idx).cloned().unwrap_or_default();
                        }
                    }
                    return String::new();
                }
            }
            self.context.get_var_or_env(inner).unwrap_or_default()
        } else if let Some(name) = s.strip_prefix('$') {
            if name.starts_with('{') {
                return s.to_string();
            }
            self.context.get_var_or_env(name).unwrap_or_default()
        } else {
            s.to_string()
        }
    }

    fn glob_match_cond(pattern: &str, value: &str) -> bool {
        use glob::Pattern;
        match Pattern::new(pattern) {
            Ok(glob_pattern) => glob_pattern.matches(value),
            Err(_) => pattern == value,
        }
    }
}

/// Classify an `ExecutionError` into a short kind string for observation.
fn error_kind_str(e: &ExecutionError) -> String {
    match e {
        ExecutionError::CommandNotFound(_) => "CommandNotFound",
        ExecutionError::PermissionDenied(_) => "PermissionDenied",
        ExecutionError::ExpansionError(_) => "ExpansionError",
        ExecutionError::ForkError(_) => "ForkError",
        ExecutionError::ExecError(_) => "ExecError",
        ExecutionError::WaitError(_) => "WaitError",
        ExecutionError::IoError(_) => "IoError",
        ExecutionError::InvalidUtf8 => "InvalidUtf8",
        ExecutionError::RedirectError(_) => "RedirectError",
        ExecutionError::AmbiguousRedirect => "AmbiguousRedirect",
        ExecutionError::InvalidRedirect(_) => "InvalidRedirect",
        ExecutionError::Return(_) => "Return",
        ExecutionError::Break => "Break",
        ExecutionError::Continue => "Continue",
        ExecutionError::Exit(_) => "Exit",
        ExecutionError::Errexit(_) => "Errexit",
        ExecutionError::InvalidVarDecl(_) => "InvalidVarDecl",
        ExecutionError::InvalidPipeline(_) => "InvalidPipeline",
    }
    .to_string()
}

/// Determine whether an execution error is recoverable (user can retry).
fn is_recoverable_error(e: &ExecutionError) -> bool {
    matches!(
        e,
        ExecutionError::CommandNotFound(_)
            | ExecutionError::PermissionDenied(_)
            | ExecutionError::ExpansionError(_)
            | ExecutionError::RedirectError(_)
            | ExecutionError::AmbiguousRedirect
            | ExecutionError::InvalidRedirect(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use colony_expand::Variable;

    fn exec_single(source: &str) -> Executor {
        let program = colony_parser::parse(source).expect("parse failed");
        let mut exec = Executor::new(ExpansionContext::new());
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(
            exec.execute_statement_recursive(program.statements.first().expect("missing stmt")),
        )
        .expect("execute failed");
        exec
    }

    fn exec_program(source: &str) -> Executor {
        let program = colony_parser::parse(source).expect("parse failed");
        let mut exec = Executor::new(ExpansionContext::new());
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        for stmt in &program.statements {
            rt.block_on(exec.execute_statement_recursive(stmt))
                .expect("execute failed");
        }
        exec
    }

    fn exec_program_result(source: &str) -> Result<Executor> {
        let program = colony_parser::parse(source).expect("parse failed");
        let mut exec = Executor::new(ExpansionContext::new());
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        for stmt in &program.statements {
            rt.block_on(exec.execute_statement_recursive(stmt))?;
        }
        Ok(exec)
    }

    #[test]
    fn var_decl_list_literal() {
        let exec = exec_single("var nums = [1, 2, 3]");
        match exec.context().get_variable("nums") {
            Some(Variable::Array(values)) => {
                assert_eq!(
                    values,
                    &vec!["1".to_string(), "2".to_string(), "3".to_string()]
                );
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_dict_literal() {
        let exec = exec_single(r#"var conf = {foo: "bar", ["baz"]: 2}"#);
        match exec.context().get_variable("conf") {
            Some(Variable::AssocArray(map)) => {
                assert_eq!(map.get("foo").map(String::as_str), Some("bar"));
                assert_eq!(map.get("baz").map(String::as_str), Some("2"));
            }
            other => panic!("expected assoc array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_field_access() {
        let exec = exec_single(r#"var x = {foo: 1}.foo"#);
        match exec.context().get_variable("x") {
            Some(Variable::String(value)) => assert_eq!(value, "1"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_index_access() {
        let exec = exec_single("var y = [10, 20][1]");
        match exec.context().get_variable("y") {
            Some(Variable::String(value)) => assert_eq!(value, "20"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_null_coalesce() {
        let exec = exec_single("var z = missing ?? 5");
        match exec.context().get_variable("z") {
            Some(Variable::String(value)) => assert_eq!(value, "5"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_ternary() {
        let exec = exec_single("var t = true ? 1 : 2");
        match exec.context().get_variable("t") {
            Some(Variable::String(value)) => assert_eq!(value, "1"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_tuple_literal() {
        let exec = exec_single("var tup = (1, 2)");
        match exec.context().get_variable("tup") {
            Some(Variable::Array(values)) => {
                assert_eq!(values, &vec!["1".to_string(), "2".to_string()]);
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_func_len() {
        let exec = exec_single("var n = len([1, 2, 3])");
        match exec.context().get_variable("n") {
            Some(Variable::String(value)) => assert_eq!(value, "3"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_func_upper() {
        let exec = exec_single(r#"var v = upper("hi")"#);
        match exec.context().get_variable("v") {
            Some(Variable::String(value)) => assert_eq!(value, "HI"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_func_bool() {
        let exec = exec_single(r#"var b = bool("")"#);
        match exec.context().get_variable("b") {
            Some(Variable::String(value)) => assert_eq!(value, "false"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_func_join() {
        let exec = exec_single(r#"var j = join("-", [1, 2, 3])"#);
        match exec.context().get_variable("j") {
            Some(Variable::String(value)) => assert_eq!(value, "1-2-3"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_func_split() {
        let exec = exec_single(r#"var s = split(":", "a:b:c")"#);
        match exec.context().get_variable("s") {
            Some(Variable::Array(values)) => {
                assert_eq!(
                    values,
                    &vec!["a".to_string(), "b".to_string(), "c".to_string()]
                );
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_func_keys_values() {
        let exec = exec_single(r#"var k = keys({foo: 1, bar: 2})"#);
        match exec.context().get_variable("k") {
            Some(Variable::Array(values)) => {
                assert!(values.contains(&"foo".to_string()));
                assert!(values.contains(&"bar".to_string()));
            }
            other => panic!("expected array, got {:?}", other),
        }

        let exec = exec_single(r#"var v = values({foo: 1, bar: 2})"#);
        match exec.context().get_variable("v") {
            Some(Variable::Array(values)) => {
                assert!(values.contains(&"1".to_string()));
                assert!(values.contains(&"2".to_string()));
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_method_join_split() {
        let exec = exec_single(r#"var j = [1, 2, 3].join("-")"#);
        match exec.context().get_variable("j") {
            Some(Variable::String(value)) => assert_eq!(value, "1-2-3"),
            other => panic!("expected string, got {:?}", other),
        }

        let exec = exec_single(r#"var s = "a:b:c".split(":")"#);
        match exec.context().get_variable("s") {
            Some(Variable::Array(values)) => {
                assert_eq!(
                    values,
                    &vec!["a".to_string(), "b".to_string(), "c".to_string()]
                );
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_method_keys_values() {
        let exec = exec_single(r#"var k = {foo: 1, bar: 2}.keys()"#);
        match exec.context().get_variable("k") {
            Some(Variable::Array(values)) => {
                assert!(values.contains(&"foo".to_string()));
                assert!(values.contains(&"bar".to_string()));
            }
            other => panic!("expected array, got {:?}", other),
        }

        let exec = exec_single(r#"var v = {foo: 1, bar: 2}.values()"#);
        match exec.context().get_variable("v") {
            Some(Variable::Array(values)) => {
                assert!(values.contains(&"1".to_string()));
                assert!(values.contains(&"2".to_string()));
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_optional_method_chain() {
        let exec = exec_single(r#"var k = missing?.keys()"#);
        match exec.context().get_variable("k") {
            Some(Variable::String(value)) => assert_eq!(value, ""),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_optional_index_field_chain() {
        let exec = exec_single(r#"var v = missing?.items[0].name?.len()"#);
        match exec.context().get_variable("v") {
            Some(Variable::String(value)) => assert_eq!(value, ""),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_method_on_null_errors() {
        let program = colony_parser::parse("var k = missing.keys()").expect("parse failed");
        let mut exec = Executor::new(ExpansionContext::new());
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let result = rt.block_on(
            exec.execute_statement_recursive(program.statements.first().expect("missing stmt")),
        );
        assert!(result.is_err());
    }

    #[test]
    fn var_decl_optional_call() {
        let exec = exec_single("var v = len?([1, 2, 3])");
        match exec.context().get_variable("v") {
            Some(Variable::String(value)) => assert_eq!(value, "3"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_direct_method_call() {
        let exec = exec_single("var n = items.len()");
        match exec.context().get_variable("n") {
            Some(Variable::String(value)) => assert_eq!(value, "0"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_array_method_len() {
        let exec = exec_program("var nums = [1, 2, 3]; var n = nums.len()");
        match exec.context().get_variable("n") {
            Some(Variable::String(value)) => assert_eq!(value, "3"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_dict_method_keys_values() {
        let exec = exec_program("var m = {foo: 1, bar: 2}; var k = m.keys(); var v = m.values()");
        match exec.context().get_variable("k") {
            Some(Variable::Array(values)) => {
                assert!(values.contains(&"foo".to_string()));
                assert!(values.contains(&"bar".to_string()));
            }
            other => panic!("expected array, got {:?}", other),
        }

        match exec.context().get_variable("v") {
            Some(Variable::Array(values)) => {
                assert!(values.contains(&"1".to_string()));
                assert!(values.contains(&"2".to_string()));
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_optional_dict_method_chain() {
        let exec = exec_program("var k = m?.keys(); var v = m?.values()");
        match exec.context().get_variable("k") {
            Some(Variable::String(value)) => assert_eq!(value, ""),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("v") {
            Some(Variable::String(value)) => assert_eq!(value, ""),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_optional_dict_method_chain_present() {
        let exec = exec_program("var m = {foo: 1, bar: 2}; var k = m?.keys(); var v = m?.values()");
        match exec.context().get_variable("k") {
            Some(Variable::Array(values)) => {
                assert!(values.contains(&"foo".to_string()));
                assert!(values.contains(&"bar".to_string()));
            }
            other => panic!("expected array, got {:?}", other),
        }
        match exec.context().get_variable("v") {
            Some(Variable::Array(values)) => {
                assert!(values.contains(&"1".to_string()));
                assert!(values.contains(&"2".to_string()));
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_dict_get_has_methods() {
        let exec = exec_program(
            r#"var m = {foo: 1, bar: 2}; var g = m.get("foo"); var d = m.get("missing", 9); var h = m.has("bar")"#,
        );
        match exec.context().get_variable("g") {
            Some(Variable::String(value)) => assert_eq!(value, "1"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("d") {
            Some(Variable::String(value)) => assert_eq!(value, "9"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("h") {
            Some(Variable::String(value)) => assert_eq!(value, "true"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_string_methods() {
        let exec = exec_program(
            r#"var s = "  hi  "; var t = s.trim(); var r = s.replace("hi", "yo"); var c = s.contains("hi")"#,
        );
        match exec.context().get_variable("t") {
            Some(Variable::String(value)) => assert_eq!(value, "hi"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("r") {
            Some(Variable::String(value)) => assert!(value.contains("yo")),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("c") {
            Some(Variable::String(value)) => assert_eq!(value, "true"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_list_methods() {
        let exec = exec_program(
            r#"var nums = [1, 2, 3]; var f = nums.first(); var l = nums.last(); var p = nums.push(4); var popped = nums.pop()"#,
        );
        match exec.context().get_variable("f") {
            Some(Variable::String(value)) => assert_eq!(value, "1"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("l") {
            Some(Variable::String(value)) => assert_eq!(value, "3"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("p") {
            Some(Variable::Array(values)) => {
                assert_eq!(
                    values,
                    &vec![
                        "1".to_string(),
                        "2".to_string(),
                        "3".to_string(),
                        "4".to_string()
                    ]
                );
            }
            other => panic!("expected array, got {:?}", other),
        }
        match exec.context().get_variable("popped") {
            Some(Variable::Array(values)) => {
                assert_eq!(values, &vec!["[1, 2]".to_string(), "3".to_string()]);
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_numeric_coercion() {
        let exec = exec_program(
            r#"var a = "3" + 2; var b = 10 - "4"; var c = "6" * 2; var d = "8" / 2; var e = "9" // 2; var f = "7" % 2"#,
        );
        match exec.context().get_variable("a") {
            Some(Variable::String(value)) => assert_eq!(value, "5"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("b") {
            Some(Variable::String(value)) => assert_eq!(value, "6"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("c") {
            Some(Variable::String(value)) => assert_eq!(value, "12"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("d") {
            Some(Variable::String(value)) => assert_eq!(value, "4"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("e") {
            Some(Variable::String(value)) => assert_eq!(value, "4"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("f") {
            Some(Variable::String(value)) => assert_eq!(value, "1"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_float_string_coercion() {
        let exec = exec_program(
            r#"var a = "3.5" + 2.0; var b = 10.0 - "4.25"; var c = "2.5" * 2; var d = "7.5" / 2.5"#,
        );
        match exec.context().get_variable("a") {
            Some(Variable::String(value)) => assert_eq!(value, "5.5"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("b") {
            Some(Variable::String(value)) => assert_eq!(value, "5.75"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("c") {
            Some(Variable::String(value)) => assert_eq!(value, "5"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("d") {
            Some(Variable::String(value)) => assert_eq!(value, "3"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_list_insert_remove_slice() {
        let exec = exec_program(
            r#"var nums = [1, 2, 3]; var i = nums.insert(1, 9); var r = nums.remove(0); var s = nums.slice(1, 3)"#,
        );
        match exec.context().get_variable("i") {
            Some(Variable::Array(values)) => {
                assert_eq!(
                    values,
                    &vec![
                        "1".to_string(),
                        "9".to_string(),
                        "2".to_string(),
                        "3".to_string()
                    ]
                );
            }
            other => panic!("expected array, got {:?}", other),
        }
        match exec.context().get_variable("r") {
            Some(Variable::Array(values)) => {
                assert_eq!(values, &vec!["[2, 3]".to_string(), "1".to_string()]);
            }
            other => panic!("expected array, got {:?}", other),
        }
        match exec.context().get_variable("s") {
            Some(Variable::Array(values)) => {
                assert_eq!(values, &vec!["2".to_string(), "3".to_string()]);
            }
            other => panic!("expected array, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_tuple_destructure_pop() {
        let exec = exec_program(r#"var nums = [1, 2, 3]; var (rest, last) = nums.pop()"#);
        match exec.context().get_variable("rest") {
            Some(Variable::Array(values)) => {
                assert_eq!(values, &vec!["1".to_string(), "2".to_string()]);
            }
            other => panic!("expected array, got {:?}", other),
        }
        match exec.context().get_variable("last") {
            Some(Variable::String(value)) => assert_eq!(value, "3"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_tuple_destructure_longer() {
        let exec = exec_program(r#"var (a, b, c) = (1, 2, 3)"#);
        match exec.context().get_variable("a") {
            Some(Variable::String(value)) => assert_eq!(value, "1"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("b") {
            Some(Variable::String(value)) => assert_eq!(value, "2"),
            other => panic!("expected string, got {:?}", other),
        }
        match exec.context().get_variable("c") {
            Some(Variable::String(value)) => assert_eq!(value, "3"),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_optional_chain_null() {
        let exec = exec_single(r#"var v = missing?.foo"#);
        match exec.context().get_variable("v") {
            Some(Variable::String(value)) => assert_eq!(value, ""),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn var_decl_unwrap_error() {
        let program = colony_parser::parse("var v = missing!").expect("parse failed");
        let mut exec = Executor::new(ExpansionContext::new());
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let result = rt.block_on(
            exec.execute_statement_recursive(program.statements.first().expect("missing stmt")),
        );
        assert!(result.is_err());
    }

    #[test]
    fn try_catch_handles_command_not_found() {
        let exec = exec_program(r#"try { missing_command } catch { var handled = 1 }"#);
        match exec.context().get_variable("handled") {
            Some(Variable::String(value)) => assert_eq!(value, "1"),
            other => panic!("expected handled, got {:?}", other),
        }
    }

    #[test]
    fn try_finally_runs_on_success() {
        let exec = exec_program(r#"try { var a = 1 } finally { var b = 2 }"#);
        match exec.context().get_variable("b") {
            Some(Variable::String(value)) => assert_eq!(value, "2"),
            other => panic!("expected b, got {:?}", other),
        }
    }

    #[test]
    fn try_catch_type_mismatch_propagates() {
        let result =
            exec_program_result(r#"try { missing_command } catch OtherError { var handled = 1 }"#);
        assert!(result.is_err());
    }

    #[test]
    fn expand_inline_expression_in_word() {
        use colony_expand::Expander;
        use colony_parser::Statement;

        let program = colony_parser::parse(r#"echo $[1 + 2] $["a" + "b"]"#).unwrap();
        let stmt = program.statements.first().unwrap();
        let cmd = match stmt {
            Statement::SimpleCommand(cmd) => cmd,
            Statement::List(list) => match &*list.first {
                Statement::Pipeline(pipeline) => match pipeline.elements.first() {
                    Some(element) => match &*element.command {
                        Statement::SimpleCommand(cmd) => cmd,
                        other => panic!("expected simple command, got {:?}", other),
                    },
                    None => panic!("expected pipeline element"),
                },
                other => panic!("expected pipeline, got {:?}", other),
            },
            other => panic!("expected simple command, got {:?}", other),
        };

        let mut expander = Expander::with_executor_and_expr(
            ExpansionContext::new(),
            |_, _| Ok(String::new()),
            |expr, ctx| crate::eval_expression_to_string(expr, ctx).map_err(|e| e.to_string()),
        );

        let first = expander.expand_word(&cmd.args[0]).unwrap();
        assert_eq!(first[0].text, "3");

        let second = expander.expand_word(&cmd.args[1]).unwrap();
        assert_eq!(second[0].text, "ab");
    }

    // --- Control flow behavioral tests ---
    // Note: break/continue as shell keywords are handled by colony-shell's
    // control_flow module, not colony-exec. Tests here cover what colony-exec
    // handles directly: for iteration, while conditions, and function return.

    #[test]
    fn for_loop_iterates_all_words() {
        let exec = exec_program("x=0; for i in 1 2 3; do x=$i; done");
        assert_eq!(
            exec.context().get_var_or_env("x"),
            Some("3".to_string()),
            "for loop should iterate through all words, x ends at last value"
        );
    }

    #[test]
    fn for_loop_sets_variable_each_iteration() {
        // Accumulate values to prove each iteration runs
        let exec = exec_program("result=''; for i in a b c; do result=\"${result}${i}\"; done");
        assert_eq!(
            exec.context().get_var_or_env("result"),
            Some("abc".to_string()),
        );
    }

    #[test]
    fn while_loop_terminates_on_condition() {
        let exec = exec_program("i=0; while [ $i -lt 3 ]; do i=$((i+1)); done");
        assert_eq!(exec.context().get_var_or_env("i"), Some("3".to_string()),);
    }

    #[test]
    fn while_false_body_never_runs() {
        let exec = exec_program("x=untouched; while false; do x=changed; done");
        assert_eq!(
            exec.context().get_var_or_env("x"),
            Some("untouched".to_string()),
        );
    }

    #[test]
    fn function_return_propagates_exit_code() {
        let exec = exec_program("myfunc() { return 42; }; myfunc");
        assert_eq!(exec.context().last_exit_status(), 42);
    }

    #[test]
    fn function_return_zero_on_success() {
        let exec = exec_program("myfunc() { return 0; }; myfunc");
        assert_eq!(exec.context().last_exit_status(), 0);
    }

    #[test]
    fn if_true_branch_executes() {
        let exec = exec_program("if true; then x=yes; else x=no; fi");
        assert_eq!(exec.context().get_var_or_env("x"), Some("yes".to_string()),);
    }

    #[test]
    fn if_false_takes_else_branch() {
        let exec = exec_program("if false; then x=yes; else x=no; fi");
        assert_eq!(exec.context().get_var_or_env("x"), Some("no".to_string()),);
    }
}
