//! Simple agent example
//!
//! Run with: cargo run --example simple_agent
//! Requires: ANTHROPIC_API_KEY environment variable

use colony_agent::{Agent, AgentConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Get API key from environment
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable required");

    // Create agent with default config
    let config = AgentConfig {
        system_prompt: "You are a helpful assistant for Colony Shell.".to_string(),
        ..Default::default()
    };

    let mut agent = Agent::with_config(api_key, config);

    println!("Colony Agent initialized\n");

    // Test basic interaction
    let response = agent.process("Hello! What can you help me with?").await?;

    println!("\nAgent response complete");
    println!("Response length: {} characters", response.len());

    Ok(())
}
