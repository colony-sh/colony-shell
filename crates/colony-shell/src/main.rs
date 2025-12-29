mod cli;
mod config;
mod repl;
mod shell;
mod tutorial;

#[tokio::main]
async fn main() -> miette::Result<()> {
    shell::run().await
}
