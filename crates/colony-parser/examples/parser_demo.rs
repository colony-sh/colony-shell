//! Demo of the Colony parser - Pure PEG Edition
//!
//! This example demonstrates the new pest-based parser which operates
//! directly on source text without a separate lexer phase.

use colony_parser::pest_api::parse;

fn main() {
    let examples = vec![
        "echo hello world",
        "echo hello | grep h",
        "ls -la | grep txt | sort",
        "echo one && echo two || echo three",
        "(echo subshell)",
        "{ echo brace; echo group; }",
        "var x = 42",
        "if (x > 10) { echo big }",
        "for (;;) { echo loop }",
        "function greet() { echo hello }",
    ];

    println!("Colony Shell Parser Demo - Pure PEG Architecture");
    println!("pest operates directly on source text (no lexer needed!)\n");

    for input in examples {
        println!("\n{}", "=".repeat(70));
        println!("Input:  {}", input);
        println!("{}", "-".repeat(70));

        match parse_and_show(input) {
            Ok(desc) => println!("{}", desc),
            Err(e) => println!("Error: {}", e),
        }
    }
}

fn parse_and_show(input: &str) -> Result<String, String> {
    // Pure PEG parsing - one call, no preprocessing
    let program = parse(input).map_err(|e| format!("Parse error: {}", e))?;

    Ok(describe_program(&program))
}

fn describe_program(prog: &colony_parser::Program) -> String {
    use colony_parser::Statement;

    let mut desc = format!("Program with {} statement(s):\n", prog.statements.len());

    for (i, stmt) in prog.statements.iter().enumerate() {
        desc.push_str(&format!("  [{}] ", i + 1));

        match stmt {
            Statement::SimpleCommand(cmd) => {
                desc.push_str(&format!("SimpleCommand: {} with {} arg(s)\n",
                    cmd.name.as_literal().unwrap_or("<complex>"),
                    cmd.args.len()
                ));
            }
            Statement::Pipeline(p) => {
                desc.push_str(&format!("Pipeline with {} element(s)\n", p.elements.len()));
                for (j, elem) in p.elements.iter().enumerate() {
                    if let Some(op) = elem.pipe_op {
                        desc.push_str(&format!("      [{}] command {:?} pipe\n", j + 1, op));
                    } else {
                        desc.push_str(&format!("      [{}] command\n", j + 1));
                    }
                }
            }
            Statement::List(list) => {
                desc.push_str(&format!("CommandList with {} additional command(s)\n", list.rest.len()));
                for (op, _) in &list.rest {
                    desc.push_str(&format!("      operator: {:?}\n", op));
                }
            }
            Statement::Subshell(s) => {
                desc.push_str(&format!("Subshell with {} statement(s)\n", s.body.len()));
            }
            Statement::BraceGroup(b) => {
                desc.push_str(&format!("BraceGroup with {} statement(s)\n", b.body.len()));
            }
            Statement::If(if_stmt) => {
                desc.push_str("If statement\n");
                desc.push_str(&format!("      then: {} statement(s)\n", if_stmt.then_body.len()));
                if let Some(ref else_body) = if_stmt.else_body {
                    desc.push_str(&format!("      else: {} statement(s)\n", else_body.len()));
                }
            }
            Statement::While(w) => {
                desc.push_str(&format!("While loop with {} statement(s)\n", w.body.len()));
            }
            Statement::For(f) => {
                desc.push_str(&format!("For loop: var={}, body={} statement(s)\n", f.var, f.body.len()));
            }
            Statement::VarDecl(v) => {
                desc.push_str(&format!("Variable declaration: {} = <expr>\n", v.name));
            }
            Statement::FunctionDef(func) => {
                desc.push_str(&format!("Function: {}() with {} statement(s)\n", func.name, func.body.len()));
            }
            _ => desc.push_str("Other statement type\n"),
        }
    }

    desc
}
