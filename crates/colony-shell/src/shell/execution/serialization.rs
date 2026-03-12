use colony_parser::{
    BinaryOp, CondBinaryOp, CondExpr, CondUnaryOp, Expression, ExtGlobOp, ProcessSubDirection,
    Statement, UnaryOp, VarKind, VariableRef, Word, WordPart,
};

pub(crate) fn statement_to_shell_string(stmt: &Statement) -> String {
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

    fn expr_to_string(expr: &Expression) -> String {
        match expr {
            Expression::Null(_) => "null".to_string(),
            Expression::Bool(b, _) => b.to_string(),
            Expression::Int(n, _) => n.to_string(),
            Expression::Float(f, _) => f.to_string(),
            Expression::Str(s, _) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Expression::List { items, .. } => {
                let parts: Vec<String> = items.iter().map(expr_to_string).collect();
                format!("[{}]", parts.join(", "))
            }
            Expression::Dict { entries, .. } => {
                let parts: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", expr_to_string(k), expr_to_string(v)))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Expression::Variable { name, .. } => format!("${name}"),
            Expression::Index { target, index, .. } => {
                format!("{}[{}]", expr_to_string(target), expr_to_string(index))
            }
            Expression::Binary {
                left, op, right, ..
            } => {
                let op_str = match op {
                    BinaryOp::Add => "+",
                    BinaryOp::Sub => "-",
                    BinaryOp::Mul => "*",
                    BinaryOp::Div => "/",
                    BinaryOp::IntDiv => "//",
                    BinaryOp::Mod => "%",
                    BinaryOp::Pow => "**",
                    BinaryOp::Eq => "==",
                    BinaryOp::Ne => "!=",
                    BinaryOp::Lt => "<",
                    BinaryOp::Le => "<=",
                    BinaryOp::Gt => ">",
                    BinaryOp::Ge => ">=",
                    BinaryOp::Match => "=~",
                    BinaryOp::NotMatch => "!~",
                    BinaryOp::And => "&&",
                    BinaryOp::Or => "||",
                    BinaryOp::BitAnd => "&",
                    BinaryOp::BitOr => "|",
                    BinaryOp::BitXor => "^",
                    BinaryOp::Shl => "<<",
                    BinaryOp::Shr => ">>",
                };
                format!(
                    "{} {} {}",
                    expr_to_string(left),
                    op_str,
                    expr_to_string(right)
                )
            }
            Expression::Unary { op, operand, .. } => {
                let op_str = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                    UnaryOp::BitNot => "~",
                };
                format!("{}{}", op_str, expr_to_string(operand))
            }
            Expression::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => format!(
                "{} ? {} : {}",
                expr_to_string(condition),
                expr_to_string(then_expr),
                expr_to_string(else_expr)
            ),
            Expression::NullCoalesce { left, right, .. } => {
                format!("{} ?? {}", expr_to_string(left), expr_to_string(right))
            }
            Expression::Field { target, field, .. } => {
                format!("{}.{}", expr_to_string(target), field)
            }
            Expression::FuncCall { name, args, .. } => {
                let arg_strs: Vec<String> = args.iter().map(expr_to_string).collect();
                format!("{}({})", name, arg_strs.join(", "))
            }
            Expression::MethodCall {
                target, name, args, ..
            } => {
                let arg_strs: Vec<String> = args.iter().map(expr_to_string).collect();
                format!(
                    "{}.{}({})",
                    expr_to_string(target),
                    name,
                    arg_strs.join(", ")
                )
            }
            Expression::OptionalCall { name, args, .. } => {
                let arg_strs: Vec<String> = args.iter().map(expr_to_string).collect();
                format!("{}?({})", name, arg_strs.join(", "))
            }
            Expression::OptionalChain { target, .. } => {
                format!("{}?", expr_to_string(target))
            }
            Expression::Unwrap { target, .. } => {
                format!("{}!", expr_to_string(target))
            }
            Expression::CommandSub { command, .. } => {
                format!("$({})", statement_to_shell_string(command))
            }
            Expression::Tuple { items, .. } => {
                let parts: Vec<String> = items.iter().map(expr_to_string).collect();
                format!("({})", parts.join(", "))
            }
            Expression::Lambda { params, body, .. } => {
                format!("|{}| {}", params.join(", "), expr_to_string(body))
            }
            Expression::Paren { inner, .. } => {
                format!("({})", expr_to_string(inner))
            }
        }
    }

    fn wordpart_to_string(part: &WordPart) -> String {
        match part {
            WordPart::Literal(s) => {
                let mut result = String::new();
                for c in s.chars() {
                    if " \t\n\"'$`\\!&|;<>(){}[]".contains(c) {
                        result.push('\\');
                    }
                    result.push(c);
                }
                result
            }
            WordPart::SingleQuoted(s) => {
                format!("'{}'", s.replace('\'', "'\\\''"))
            }
            WordPart::DoubleQuoted(parts) => {
                let mut result = String::from('"');
                for p in parts {
                    result.push_str(&wordpart_in_dquote_to_string(p));
                }
                result.push('"');
                result
            }
            WordPart::AnsiCQuoted(s) => format!("$'{s}'"),
            WordPart::Variable(v) => varref_to_string(v),
            WordPart::CommandSub(stmt) => {
                format!("$({})", statement_to_shell_string(stmt))
            }
            WordPart::CommandSubRaw(s) => format!("$({s})"),
            WordPart::ArithRaw(s) => format!("$(({s}))"),
            WordPart::ExprSub(expr) => format!("$[{}]", expr_to_string(expr)),
            WordPart::Tilde(user) => match user {
                Some(u) => format!("~{u}"),
                None => "~".to_string(),
            },
            WordPart::BraceExpansion(s) => format!("{{{s}}}"),
            WordPart::ProcessSub { direction, command } => match direction {
                ProcessSubDirection::Input => format!("<({command})"),
                ProcessSubDirection::Output => format!(">({command})"),
            },
            WordPart::ExtGlob {
                operator,
                pattern_list,
            } => {
                let op_char = match operator {
                    ExtGlobOp::ZeroOrOne => '?',
                    ExtGlobOp::ZeroOrMore => '*',
                    ExtGlobOp::OneOrMore => '+',
                    ExtGlobOp::ExactlyOne => '@',
                    ExtGlobOp::Not => '!',
                };
                format!("{op_char}({pattern_list})")
            }
        }
    }

    fn wordpart_in_dquote_to_string(part: &WordPart) -> String {
        match part {
            WordPart::Literal(s) => {
                let mut result = String::new();
                for c in s.chars() {
                    if "\"$`\\".contains(c) {
                        result.push('\\');
                    }
                    result.push(c);
                }
                result
            }
            WordPart::Variable(v) => varref_to_string(v),
            WordPart::ExprSub(expr) => format!("$[{}]", expr_to_string(expr)),
            WordPart::CommandSub(stmt) => {
                format!("$({})", statement_to_shell_string(stmt))
            }
            WordPart::CommandSubRaw(s) => format!("$({s})"),
            WordPart::ArithRaw(s) => format!("$(({s}))"),
            WordPart::Tilde(user) => match user {
                Some(u) => format!("~{u}"),
                None => "~".to_string(),
            },
            // Inside double quotes, these appear verbatim
            WordPart::SingleQuoted(s) => format!("'{}'", s.replace('\'', "'\\\''").replace('"', "\\\"")),
            WordPart::AnsiCQuoted(s) => format!("$'{s}'"),
            WordPart::BraceExpansion(s) => format!("{{{s}}}"),
            WordPart::ProcessSub { direction, command } => match direction {
                ProcessSubDirection::Input => format!("<({command})"),
                ProcessSubDirection::Output => format!(">({command})"),
            },
            WordPart::ExtGlob {
                operator,
                pattern_list,
            } => {
                let op_char = match operator {
                    ExtGlobOp::ZeroOrOne => '?',
                    ExtGlobOp::ZeroOrMore => '*',
                    ExtGlobOp::OneOrMore => '+',
                    ExtGlobOp::ExactlyOne => '@',
                    ExtGlobOp::Not => '!',
                };
                format!("{op_char}({pattern_list})")
            }
            // Nested double-quoted parts are unlikely but handle gracefully
            WordPart::DoubleQuoted(parts) => {
                let mut result = String::new();
                for p in parts {
                    result.push_str(&wordpart_in_dquote_to_string(p));
                }
                result
            }
        }
    }

    fn word_to_string(word: &Word) -> String {
        let mut result = String::new();
        for part in &word.parts {
            result.push_str(&wordpart_to_string(part));
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
        stmts
            .iter()
            .map(statement_to_shell_string)
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn condition_to_string(cond: &colony_parser::Condition) -> String {
        match cond {
            colony_parser::Condition::Command(stmt) => statement_to_shell_string(stmt),
            colony_parser::Condition::Expression(expr) => format!("({})", expr_to_string(expr)),
        }
    }

    fn cond_expr_to_string(expr: &CondExpr) -> String {
        match expr {
            CondExpr::And(left, right) => {
                format!(
                    "{} && {}",
                    cond_expr_to_string(left),
                    cond_expr_to_string(right)
                )
            }
            CondExpr::Or(left, right) => {
                format!(
                    "{} || {}",
                    cond_expr_to_string(left),
                    cond_expr_to_string(right)
                )
            }
            CondExpr::Not(inner) => format!("! {}", cond_expr_to_string(inner)),
            CondExpr::Binary {
                left, op, right, ..
            } => {
                let op_str = match op {
                    CondBinaryOp::Eq => "==",
                    CondBinaryOp::Ne => "!=",
                    CondBinaryOp::Match => "=~",
                    CondBinaryOp::Lt => "<",
                    CondBinaryOp::Gt => ">",
                    CondBinaryOp::IntEq => "-eq",
                    CondBinaryOp::IntNe => "-ne",
                    CondBinaryOp::IntLt => "-lt",
                    CondBinaryOp::IntLe => "-le",
                    CondBinaryOp::IntGt => "-gt",
                    CondBinaryOp::IntGe => "-ge",
                    CondBinaryOp::NewerThan => "-nt",
                    CondBinaryOp::OlderThan => "-ot",
                    CondBinaryOp::SameFile => "-ef",
                };
                format!("{left} {op_str} {right}")
            }
            CondExpr::Unary { op, arg } => {
                let op_str = match op {
                    CondUnaryOp::NonZeroLen => "-n",
                    CondUnaryOp::ZeroLen => "-z",
                    CondUnaryOp::IsFile => "-f",
                    CondUnaryOp::IsDir => "-d",
                    CondUnaryOp::Exists => "-e",
                    CondUnaryOp::Readable => "-r",
                    CondUnaryOp::Writable => "-w",
                    CondUnaryOp::Executable => "-x",
                    CondUnaryOp::NonEmpty => "-s",
                    CondUnaryOp::IsSymlink => "-L",
                    CondUnaryOp::IsPipe => "-p",
                    CondUnaryOp::IsSocket => "-S",
                    CondUnaryOp::IsBlock => "-b",
                    CondUnaryOp::IsChar => "-c",
                    CondUnaryOp::IsTerminal => "-t",
                    CondUnaryOp::OwnedByEuid => "-O",
                    CondUnaryOp::OwnedByEgid => "-G",
                    CondUnaryOp::ModifiedSinceRead => "-N",
                    CondUnaryOp::VarIsSet => "-v",
                };
                format!("{op_str} {arg}")
            }
            CondExpr::String(s) => s.clone(),
        }
    }

    fn match_pattern_to_string(pattern: &colony_parser::MatchPattern) -> String {
        match pattern {
            colony_parser::MatchPattern::Literal(expr) => expr_to_string(expr),
            colony_parser::MatchPattern::Binding(name) => name.clone(),
            colony_parser::MatchPattern::Wildcard => "_".to_string(),
            colony_parser::MatchPattern::List(list) => {
                let mut parts: Vec<String> =
                    list.items.iter().map(match_pattern_to_string).collect();
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
                        parts.push(format!(
                            "{}: {}",
                            field.name,
                            match_pattern_to_string(pattern)
                        ));
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
                let parts: Vec<String> = tuple.items.iter().map(match_pattern_to_string).collect();
                format!("({})", parts.join(", "))
            }
            colony_parser::MatchPattern::Or(patterns) => {
                let parts: Vec<String> = patterns.iter().map(match_pattern_to_string).collect();
                parts.join(" | ")
            }
        }
    }

    match stmt {
        Statement::SimpleCommand(cmd) => simple_cmd_to_string(cmd),
        Statement::Pipeline(pipe) => pipe
            .elements
            .iter()
            .map(|elem| statement_to_shell_string(&elem.command))
            .collect::<Vec<_>>()
            .join(" | "),
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
        Statement::BraceGroup(bg) => format!("{{ {}; }}", statements_to_string(&bg.body)),
        Statement::Subshell(sub) => format!("( {} )", statements_to_string(&sub.body)),
        Statement::If(if_stmt) => {
            let mut result = format!(
                "if {}; then {}",
                condition_to_string(&if_stmt.condition),
                statements_to_string(&if_stmt.then_body)
            );
            for elif in &if_stmt.elif_branches {
                result.push_str(&format!(
                    "; elif {}; then {}",
                    condition_to_string(&elif.condition),
                    statements_to_string(&elif.body)
                ));
            }
            if let Some(else_body) = &if_stmt.else_body {
                result.push_str(&format!("; else {}", statements_to_string(else_body)));
            }
            result.push_str("; fi");
            result
        }
        Statement::Try(try_stmt) => {
            let mut result = format!("try {{ {} }}", statements_to_string(&try_stmt.body));
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
                result.push_str(&format!(
                    " {{ {} }}",
                    statements_to_string(&catch_block.body)
                ));
            }
            if let Some(finally_body) = &try_stmt.finally_body {
                result.push_str(&format!(
                    " finally {{ {} }}",
                    statements_to_string(finally_body)
                ));
            }
            result
        }
        Statement::Match(match_stmt) => {
            let mut result = format!("match {} {{ ", expr_to_string(&match_stmt.value));
            for arm in &match_stmt.arms {
                result.push_str(&match_pattern_to_string(&arm.pattern));
                if let Some(guard) = &arm.guard {
                    result.push_str(&format!(" if {}", expr_to_string(guard)));
                }
                result.push_str(" => ");
                match &arm.body {
                    colony_parser::MatchBody::Block(stmts) => {
                        result.push_str(&format!("{{ {} }} ", statements_to_string(stmts)));
                    }
                    colony_parser::MatchBody::Expr(expr) => {
                        result.push_str(&format!("{} ", expr_to_string(expr)));
                    }
                }
            }
            result.push('}');
            result
        }
        Statement::While(while_stmt) => format!(
            "while {}; do {}; done",
            condition_to_string(&while_stmt.condition),
            statements_to_string(&while_stmt.body)
        ),
        Statement::Until(until_stmt) => format!(
            "until {}; do {}; done",
            condition_to_string(&until_stmt.condition),
            statements_to_string(&until_stmt.body)
        ),
        Statement::For(for_stmt) => {
            let words: Vec<String> = for_stmt.words.iter().map(word_to_string).collect();
            format!(
                "for {} in {}; do {}; done",
                for_stmt.var,
                words.join(" "),
                statements_to_string(&for_stmt.body)
            )
        }
        Statement::CStyleFor(for_stmt) => {
            let init = for_stmt.init.as_deref().unwrap_or("");
            let cond = for_stmt.condition.as_deref().unwrap_or("");
            let update = for_stmt.update.as_deref().unwrap_or("");
            format!(
                "for (({init}; {cond}; {update})); do {}; done",
                statements_to_string(&for_stmt.body)
            )
        }
        Statement::Case(case_stmt) => {
            let word = word_to_string(&case_stmt.value);
            let mut result = format!("case {} in ", word);
            for arm in &case_stmt.arms {
                let patterns: Vec<String> = arm.patterns.iter().map(word_to_string).collect();
                result.push_str(&format!(
                    "{}) {};; ",
                    patterns.join("|"),
                    statements_to_string(&arm.body)
                ));
            }
            result.push_str("esac");
            result
        }
        Statement::Conditional(cond) => {
            format!("[[ {} ]]", cond_expr_to_string(&cond.expr))
        }
        Statement::Arithmetic(arith) => format!("(( {} ))", arith.expr),
        Statement::Coproc(coproc) => {
            let cmd = statement_to_shell_string(&coproc.command);
            match &coproc.name {
                Some(name) => format!("coproc {name} {cmd}"),
                None => format!("coproc {cmd}"),
            }
        }
        Statement::VarDecl(decl) => {
            let keyword = match decl.kind {
                VarKind::Var => "var",
                VarKind::Const => "const",
                VarKind::Setvar => "setvar",
            };
            let target = match &decl.target {
                colony_parser::VarDeclTarget::Name(name) => name.clone(),
                colony_parser::VarDeclTarget::Tuple(names) => {
                    format!("({})", names.join(", "))
                }
            };
            let type_ann = decl
                .type_annotation
                .as_ref()
                .map_or(String::new(), |t| format!(": {t}"));
            format!(
                "{keyword} {target}{type_ann} = {}",
                expr_to_string(&decl.value)
            )
        }
        Statement::FunctionDef(func) => {
            format!(
                "{name}() {{ {body}; }}",
                name = func.name,
                body = statements_to_string(&func.body)
            )
        }
        Statement::ProcDef(proc_def) => {
            let params: Vec<String> = proc_def
                .params
                .iter()
                .map(|p| match &p.default {
                    Some(def) => format!("{} = {}", p.name, expr_to_string(def)),
                    None => p.name.clone(),
                })
                .collect();
            format!(
                "proc {name}({params}) {{ {body}; }}",
                name = proc_def.name,
                params = params.join(", "),
                body = statements_to_string(&proc_def.body)
            )
        }
        Statement::AgentInvocation(agent) => {
            let tool = agent
                .tool
                .as_ref()
                .map_or(String::new(), |t| format!(":{t}"));
            format!(
                "@{name}{tool} {prompt}",
                name = agent.agent_name,
                prompt = word_to_string(&agent.prompt)
            )
        }
        Statement::Return(ret) => match &ret.value {
            Some(expr) => format!("return {}", expr_to_string(expr)),
            None => "return".to_string(),
        },
        Statement::Empty => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::statement_to_shell_string;
    use colony_parser::pest_api::parse_statement;

    #[test]
    fn round_trips_simple_command_to_shell_string() {
        let stmt = parse_statement("echo hello").expect("parse simple command");
        let rendered = statement_to_shell_string(&stmt);
        assert_eq!(rendered, "echo hello");
    }
}
