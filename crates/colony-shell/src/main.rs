//! Colony Shell - POSIX-compliant shell for AI-native terminal

use miette::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    println!("Colony Shell v{}", env!("CARGO_PKG_VERSION"));
    println!("Type 'exit' to quit");

    // TODO: Implement REPL
    // For now, just a placeholder

    Ok(())
}
