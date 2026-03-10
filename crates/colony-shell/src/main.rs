mod cli;
mod config;
#[allow(dead_code)]
mod event_worker;
#[cfg_attr(feature = "tui", allow(dead_code, unused_imports))]
mod repl;
#[cfg(feature = "tui")]
mod repl_tui;
mod shell;
mod tutorial;
mod widget_handler;
mod widget_server;

#[tokio::main]
async fn main() -> miette::Result<()> {
    shell::run().await
}
