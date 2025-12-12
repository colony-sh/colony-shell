//! Colony Shell - POSIX-compliant shell for AI-native terminal

use colony_agent::{Agent, AgentConfig, NaturalLanguageDetector};
use colony_parser::{Lexer, Parser};
use miette::Result;
use rustyline::{error::ReadlineError, DefaultEditor};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    println!("Colony Shell v{}", env!("CARGO_PKG_VERSION"));
    println!("POSIX-compliant shell with AI agent integration");
    println!();

    // Initialize agent if API key is available
    let agent = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .map(|key| {
            let config = AgentConfig {
                system_prompt: "You are Colony Shell's AI assistant. You help users with \
                                shell commands, system administration, and programming tasks."
                    .to_string(),
                use_caching: true,
                ..Default::default()
            };
            Agent::with_config(key, config)
        });

    if agent.is_some() {
        println!("🤖 Agent mode enabled (ANTHROPIC_API_KEY found)");
        println!("   Use # or ? prefix for natural language queries");
    } else {
        println!("⚠️  Agent mode disabled (set ANTHROPIC_API_KEY to enable)");
    }

    println!("   Type 'exit' to quit\n");

    // Start REPL
    run_repl(agent).await
}

async fn run_repl(mut agent: Option<Agent>) -> Result<()> {
    let mut rl = DefaultEditor::new()
        .map_err(|e| miette::miette!("failed to create readline: {}", e))?;

    loop {
        let prompt = if agent.is_some() { "colony> " } else { "shell> " };

        let readline = rl.readline(prompt);

        match readline {
            Ok(line) => {
                let line = line.trim();

                if line.is_empty() {
                    continue;
                }

                // Add to history
                rl.add_history_entry(line)
                    .map_err(|e| miette::miette!("history error: {}", e))?;

                // Handle exit
                if line == "exit" || line == "quit" {
                    println!("Goodbye!");
                    break;
                }

                // Route to agent or shell
                if let Some(ref mut agent) = agent {
                    if NaturalLanguageDetector::is_natural_language(line) {
                        // Route to agent
                        match agent.process(line).await {
                            Ok(_response) => {
                                println!(); // Newline after streaming
                            }
                            Err(e) => {
                                eprintln!("Agent error: {}", e);
                            }
                        }
                        continue;
                    }
                }

                // Try parsing as shell command
                match execute_shell_command(line) {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }

            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }

            Err(ReadlineError::Eof) => {
                println!("exit");
                break;
            }

            Err(err) => {
                eprintln!("Error: {}", err);
                break;
            }
        }
    }

    Ok(())
}

fn execute_shell_command(line: &str) -> Result<()> {
    // Parse the command
    let mut lexer = Lexer::new(line);
    let tokens = lexer
        .tokenize()
        .map_err(|e| miette::miette!("lexer error: {}", e))?;

    let mut parser = Parser::new(tokens);
    let _ast = parser
        .parse()
        .map_err(|e| miette::miette!("parser error: {}", e))?;

    // TODO: Execute the parsed command
    println!("(Shell execution not yet implemented)");

    Ok(())
}
