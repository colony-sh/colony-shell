use colony_parser::{Statement, VariableRef, Word, WordPart};

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
                    result.push_str(&s.replace('\'', "'\\\''"));
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
                other => result.push_str(&format!("{:?}", other)),
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
        stmts
            .iter()
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
        Statement::Empty => String::new(),
        other => format!("{:?}", other),
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
