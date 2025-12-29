mod tutorial;
mod cli;
mod config;
mod repl;
mod shell;

#[tokio::main]
async fn main() -> miette::Result<()> {
    shell::run().await
}
