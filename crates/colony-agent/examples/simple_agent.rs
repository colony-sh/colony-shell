//! Simple agent example
//!
//! Run with: cargo run --example simple_agent
//! Requires: ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable (or local Ollama)

use colony_agent::{Agent, AgentConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create agent with default config
    // Provider is auto-detected: Anthropic → OpenAI → Ollama
    let config = AgentConfig {
        system_prompt: "You are a helpful assistant for Colony Shell.".to_string(),
        ..Default::default()
    };

    let mut agent = Agent::with_config(config).await?;

    println!("Colony Agent initialized\n");

    let response = agent.process("Hello! What can you help me with?").await?;

    println!("\nAgent response complete");
    println!("Response length: {} characters", response.len());

    Ok(())
}
