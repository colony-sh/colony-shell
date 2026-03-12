use colony_agent::{Agent, AgentConfig, ModelRouter, ProviderType};

/// Handle `llm` subcommands for model management and direct LLM calls.
///
/// Subcommands:
///   llm status         - Show current model info
///   llm list           - List available providers
///   llm use <p:m>      - Switch model
///   llm ask <prompt>   - Direct LLM call with prompt text
///   llm <prompt>       - Shorthand for `llm ask <prompt>`
pub(crate) async fn handle_llm_command(line: &str, agent: &mut Option<Agent>) {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 2 {
        print_llm_usage();
        return;
    }

    let subcommand = parts[1];

    match subcommand {
        "status" => handle_status(agent),
        "list" => handle_list().await,
        "use" => handle_use(&parts, agent).await,
        "ask" => {
            // `llm ask <prompt text...>`
            if parts.len() < 3 {
                eprintln!("Usage: llm ask <prompt>");
                return;
            }
            let prompt = parts[2..].join(" ");
            handle_ask(&prompt, agent).await;
        }
        "help" | "--help" | "-h" => print_llm_usage(),
        _ => {
            // Default: treat everything after `llm` as a direct prompt
            // `llm What is Rust?` -> direct LLM call
            let prompt = parts[1..].join(" ");
            handle_ask(&prompt, agent).await;
        }
    }
}

fn print_llm_usage() {
    eprintln!("Usage: llm <subcommand|prompt>");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  status              Show current model provider and model");
    eprintln!("  list                List available providers");
    eprintln!("  use <provider:model>  Switch to a different model");
    eprintln!("  ask <prompt>        Send a prompt directly to the LLM");
    eprintln!("  help                Show this help message");
    eprintln!();
    eprintln!("Direct call (shorthand for `llm ask`):");
    eprintln!("  llm What is Rust?");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  llm use anthropic:claude-sonnet-4-20250514");
    eprintln!("  llm use openai:gpt-4-turbo");
    eprintln!("  llm use ollama:llama3");
    eprintln!("  llm ask Explain monads in 3 sentences");
}

fn handle_status(agent: &Option<Agent>) {
    if let Some(ref agent) = agent {
        let (provider, model) = agent.current_provider();
        println!("Provider: {:?}", provider);
        println!("Model: {}", model);
        println!("System prompt: {}", truncate(agent.system_prompt(), 80));
        println!("Max tokens: {}", agent.config().max_tokens);
        if let Some(temp) = agent.config().temperature {
            println!("Temperature: {:.1}", temp);
        }
    } else {
        println!("Agent not initialized (no API key or AGENT.md found)");
    }
}

async fn handle_list() {
    println!("Available providers:");
    let providers = ModelRouter::list_available_providers().await;

    for info in providers {
        let status = if info.available { "[ok]" } else { "[--]" };
        println!("  {} {:?}: {}", status, info.provider_type, info.model);
        if let Some(reason) = info.reason {
            println!("      {}", reason);
        }
    }
}

async fn handle_use(parts: &[&str], agent: &mut Option<Agent>) {
    if parts.len() < 3 {
        eprintln!("Usage: llm use <provider>:<model>");
        eprintln!("Example: llm use anthropic:claude-sonnet-4-20250514");
        eprintln!("Example: llm use openai:gpt-4-turbo");
        eprintln!("Example: llm use ollama:llama3");
        return;
    }

    let spec = parts[2];
    let Some((provider_str, model_name)) = spec.split_once(':') else {
        eprintln!("Invalid format. Use: provider:model");
        return;
    };

    let Some(provider_type) = ProviderType::parse(provider_str) else {
        eprintln!("Unknown provider: {}", provider_str);
        eprintln!("Available: anthropic, openai, ollama");
        return;
    };

    if let Some(ref mut agent) = agent {
        match agent.switch_model(provider_type, model_name).await {
            Ok(()) => {
                println!("Switched to {:?}: {}", provider_type, model_name);
            }
            Err(e) => {
                eprintln!("Failed to switch model: {}", e);
            }
        }
    } else {
        eprintln!("Agent not initialized - cannot switch model");
    }
}

/// Handle a direct LLM prompt: send text to the model and stream the response.
async fn handle_ask(prompt: &str, agent: &mut Option<Agent>) {
    // If no agent, try to create one on-the-fly
    if agent.is_none() {
        match Agent::with_config(AgentConfig {
            system_prompt: "You are a helpful assistant in a shell environment. Be concise."
                .to_string(),
            ..AgentConfig::default()
        })
        .await
        {
            Ok(new_agent) => {
                println!(
                    "[Using {:?}: {}]",
                    new_agent.current_provider().0,
                    new_agent.current_provider().1,
                );
                *agent = Some(new_agent);
            }
            Err(e) => {
                eprintln!("Failed to initialize LLM: {}", e);
                eprintln!("Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or start ollama");
                return;
            }
        }
    }

    let Some(ref mut agent) = agent else {
        // Unreachable due to above initialization, but satisfies the compiler
        return;
    };

    match agent.process(prompt).await {
        Ok(_response) => {
            // Response was already streamed to stdout via emit_thought()
            println!();
        }
        Err(e) => {
            eprintln!("LLM error: {}", e);
        }
    }
}

/// Truncate a string to a maximum length, appending "..." if truncated
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("hello world, this is a long string", 10);
        assert!(result.len() <= 10);
        assert!(result.ends_with("..."));
    }
}
