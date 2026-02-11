use colony_agent::AgentConfig;
use colony_config::{discover_config, ColonyConfig, TrustManager, TrustStatus};
use std::io::{self, Write};

pub(crate) fn build_agent_config(colony_config: &Option<ColonyConfig>) -> AgentConfig {
    let mut system_prompt = String::from(
        "You are Colony Shell's AI assistant. You help users with shell commands, system administration, and programming tasks.",
    );

    let use_caching = true;
    let mut max_tokens = 4096;
    let mut temperature = None;

    if let Some(ref config) = colony_config {
        if let Some(ref agent_cfg) = config.agent {
            if let Some(ref personality) = agent_cfg.personality {
                system_prompt = personality.clone();
            }

            if let Some(temp) = agent_cfg.temperature {
                temperature = Some(temp);
            }
            if let Some(tokens) = agent_cfg.max_tokens {
                max_tokens = tokens;
            }
        }

        if let Some(ref project) = config.project {
            system_prompt.push_str("\n\n## Project Context\n");

            if let Some(ref name) = project.name {
                system_prompt.push_str(&format!("Project: {}\n", name));
            }
            if let Some(ref desc) = project.description {
                system_prompt.push_str(&format!("Description: {}\n", desc));
            }
            if let Some(ref lang) = project.language {
                system_prompt.push_str(&format!("Language: {}\n", lang));
            }

            if !project.goals.is_empty() {
                system_prompt.push_str("\n### Project Goals\n");
                for goal in &project.goals {
                    system_prompt.push_str(&format!("- {}\n", goal));
                }
            }

            if !project.conventions.is_empty() {
                system_prompt.push_str("\n### Coding Conventions\n");
                for conv in &project.conventions {
                    system_prompt.push_str(&format!("- {}\n", conv));
                }
            }

            if !project.docs.is_empty() {
                system_prompt.push_str("\n### Documentation\n");
                for doc in &project.docs {
                    system_prompt.push_str(&format!("- {}\n", doc));
                }
            }
        }
    }

    AgentConfig {
        system_prompt,
        use_caching,
        max_tokens,
        temperature,
        ..Default::default()
    }
}

pub(crate) async fn load_colonyrc_with_trust() -> Option<ColonyConfig> {
    let location = match discover_config() {
        Ok(Some(loc)) => loc,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("⚠️  Config discovery error: {}", e);
            return None;
        }
    };

    match location.trust_status {
        TrustStatus::Trusted { .. } => {
            println!("📋 Loaded .colonyrc from: {}", location.path.display());
            Some(location.config)
        }

        TrustStatus::Untrusted { ref path, ref hash } => {
            println!();
            println!("⚠️  Found .colonyrc but directory not trusted");
            println!("   Path: {}", path.display());
            println!("   Hash: {}...", &hash[..16]);
            println!();
            println!("   This config will modify agent behavior.");
            print!("   Trust this directory? [y/N/show]: ");
            io::stdout().flush().ok();

            let mut response = String::new();
            if io::stdin().read_line(&mut response).is_ok() {
                match response.trim().to_lowercase().as_str() {
                    "y" | "yes" => {
                        let mut trust_mgr = TrustManager::new().ok()?;
                        let content = std::fs::read_to_string(path).ok()?;

                        if trust_mgr.trust(path, &content).is_ok() {
                            println!("   ✓ Trusted and loaded .colonyrc");
                            Some(location.config)
                        } else {
                            eprintln!("   ✗ Failed to save trust");
                            None
                        }
                    }
                    "show" => {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            println!("\n--- .colonyrc content ---");
                            println!("{}", content);
                            println!("--- end ---\n");
                        }
                        None
                    }
                    _ => {
                        println!("   Config not loaded");
                        None
                    }
                }
            } else {
                None
            }
        }

        TrustStatus::Changed {
            ref path,
            ref current_hash,
            ref trusted_hash,
        } => {
            println!();
            println!("⚠️  .colonyrc has changed since last trust");
            println!("   Path: {}", path.display());
            println!("   Old hash: {}...", &trusted_hash[..16]);
            println!("   New hash: {}...", &current_hash[..16]);
            println!();
            print!("   Re-trust? [y/N/show]: ");
            io::stdout().flush().ok();

            let mut response = String::new();
            if io::stdin().read_line(&mut response).is_ok() {
                match response.trim().to_lowercase().as_str() {
                    "y" | "yes" => {
                        let mut trust_mgr = TrustManager::new().ok()?;
                        let content = std::fs::read_to_string(path).ok()?;

                        if trust_mgr.trust(path, &content).is_ok() {
                            println!("   ✓ Re-trusted and loaded .colonyrc");
                            Some(location.config)
                        } else {
                            eprintln!("   ✗ Failed to save trust");
                            None
                        }
                    }
                    "show" => {
                        if let Ok(content) = std::fs::read_to_string(path) {
                            println!("\n--- .colonyrc content ---");
                            println!("{}", content);
                            println!("--- end ---\n");
                        }
                        None
                    }
                    _ => {
                        println!("   Config not loaded");
                        None
                    }
                }
            } else {
                None
            }
        }

        TrustStatus::NotFound => None,
    }
}
