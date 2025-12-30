mod cli;
mod config;
mod repl;
mod shell;
mod tutorial;
mod widget_handler;

#[tokio::main]
async fn main() -> miette::Result<()> {
    shell::run().await
}
