use colony_agent::{Agent, ModelRouter, ProviderType};

/// Handle `llm` subcommands for model management (status, list, use).
pub(crate) async fn handle_llm_command(line: &str, agent: &mut Option<Agent>) {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 2 {
        eprintln!("Usage: llm <status|list|use>");
        return;
    }

    let subcommand = parts[1];

    match subcommand {
        "status" => {
            if let Some(ref agent) = agent {
                let (provider, model) = agent.current_provider();
                println!("Current provider: {:?}", provider);
                println!("Current model: {}", model);
            } else {
                println!("Agent not initialized");
            }
        }

        "list" => {
            println!("Available providers:");
            let providers = ModelRouter::list_available_providers().await;

            for info in providers {
                let status = if info.available { "✓" } else { "✗" };
                println!("  {} {:?}: {}", status, info.provider_type, info.model);
                if let Some(reason) = info.reason {
                    println!("      {}", reason);
                }
            }
        }

        "use" => {
            if parts.len() < 3 {
                eprintln!("Usage: llm use <provider>:<model>");
                eprintln!("Example: llm use anthropic:claude-3-5-sonnet-20241022");
                eprintln!("Example: llm use openai:gpt-4-turbo");
                eprintln!("Example: llm use ollama:llama3");
                return;
            }

            let spec = parts[2];
            let spec_parts: Vec<&str> = spec.split(':').collect();
            if spec_parts.len() != 2 {
                eprintln!("Invalid format. Use: provider:model");
                return;
            }

            let provider_str = spec_parts[0];
            let model_name = spec_parts[1];

            let provider_type = match ProviderType::from_str(provider_str) {
                Some(p) => p,
                None => {
                    eprintln!("Unknown provider: {}", provider_str);
                    eprintln!("Available: anthropic, openai, ollama");
                    return;
                }
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

        _ => {
            eprintln!("Unknown subcommand: {}", subcommand);
            eprintln!("Available: status, list, use");
        }
    }
}
