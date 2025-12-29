//! Demo of the Colony parser

use colony_parser::parse;

fn main() {
    let examples = vec![
        "echo hello world",
        "echo hello | grep h",
        "ls -la | grep txt | sort",
        "echo one && echo two || echo three",
        "(echo subshell)",
        "{ echo brace; echo group; }",
    ];

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
            _ => desc.push_str("Other statement type\n"),
        }
    }

    desc
}
