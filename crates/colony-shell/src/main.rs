mod cli;
mod config;
mod repl;
mod shell;
mod tutorial;
mod widget_handler;
mod widget_server;

#[tokio::main]
async fn main() -> miette::Result<()> {
    shell::run().await
}
