use std::sync::Arc;

use crate::cli::ShellOptions;
use crate::config::{build_agent_config, load_colonyrc_with_trust};
#[cfg(not(feature = "tui"))]
use crate::repl::run_repl;
use crate::shell::engine::execute_command_with_status;
use crate::shell::startup::{init_secure_registry, load_exported_functions, run_startup_files};
use crate::widget_server::WidgetServer;
use colony_agent::Agent;
use colony_exec::Executor;
use colony_expand::ExpansionContext;
use colony_reflection::ReflectionObserver;
use miette::Result;
use tokio::{pin, select, signal};
use tracing_subscriber::EnvFilter;

fn create_observer() -> Arc<dyn ReflectionObserver> {
    // Inside colony-nerve terminal widget: forward via OSC 2337
    if std::env::var("COLONY_NERVE_TERMINAL").is_ok() {
        tracing::info!("Reflection observer: OSC mode (inside colony-nerve)");
        return Arc::new(colony_reflection::OscObserver::new());
    }

    // Standalone: use local SQLite
    let session_id = std::process::id().to_string();
    match colony_reflection::SqliteObserver::new(session_id) {
        Ok(observer) => {
            tracing::info!("Reflection observer: SQLite mode (standalone)");
            Arc::new(observer)
        }
        Err(e) => {
            tracing::warn!("Failed to initialize reflection observer: {}", e);
            Arc::new(colony_reflection::NoopObserver)
        }
    }
}

pub(crate) async fn run() -> Result<()> {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let opts = ShellOptions::parse();

    if let Some(ref command) = opts.command {
        run_command_mode(command, &opts).await;
    }

    if let Some(ref script) = opts.script {
        run_script_mode(script, &opts).await;
    }

    run_repl_mode(&opts).await
}

async fn run_command_mode(command: &str, opts: &ShellOptions) -> ! {
    let mut context = ExpansionContext::new();
    load_exported_functions(&mut context);
    init_secure_registry(&mut context);
    if !opts.positional_args.is_empty() {
        context.set_positional_params(opts.positional_args.clone());
    }

    let mut executor = Executor::new(context);
    executor.set_observer(create_observer());
    run_startup_files(opts, &mut executor).await;

    let exec_future = execute_command_with_status(command, &mut executor);
    pin!(exec_future);

    select! {
        _ = signal::ctrl_c() => {
            eprintln!("colony: interrupted (SIGINT)");
            std::process::exit(130);
        }
        code = &mut exec_future => {
            std::process::exit(code);
        }
    }
}

async fn run_script_mode(script: &std::path::Path, opts: &ShellOptions) -> ! {
    if opts.tutorial {
        use crate::tutorial::tracer::TraceDepth;
        use crate::tutorial::{TutorialConfig, TutorialRunner};

        let depth = match opts.depth {
            3 => TraceDepth::Internals,
            _ => TraceDepth::Mechanics,
        };

        let config = TutorialConfig {
            depth,
            teach_mode: opts.teach,
            pause_on_complex: true,
        };

        let mut runner = match TutorialRunner::new(script, config) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("colony: tutorial error: {}", e);
                std::process::exit(1);
            }
        };

        match runner.run() {
            Ok(exit_code) => std::process::exit(exit_code),
            Err(e) => {
                eprintln!("colony: tutorial error: {}", e);
                std::process::exit(1);
            }
        }
    }

    let mut context = ExpansionContext::new();
    load_exported_functions(&mut context);
    init_secure_registry(&mut context);
    context.set_shell_name(script.display().to_string());
    context.set_current_script_path(Some(script.to_path_buf()));
    context.set_positional_params(opts.positional_args.clone());

    let mut executor = Executor::new(context);
    executor.set_observer(create_observer());
    run_startup_files(opts, &mut executor).await;

    let exec_future = async {
        match executor.execute_source_file(script, vec![]).await {
            Ok(status) => status.code(),
            Err(e) => {
                eprintln!("colony: {}: {}", script.display(), e);
                127
            }
        }
    };

    select! {
        _ = signal::ctrl_c() => {
            eprintln!("colony: interrupted (SIGINT)");
            std::process::exit(130);
        }
        code = exec_future => {
            std::process::exit(code);
        }
    }
}

async fn run_repl_mode(_opts: &ShellOptions) -> Result<()> {
    println!("Colony Shell v{}", env!("CARGO_PKG_VERSION"));
    println!("POSIX-compliant shell with AI agent integration");
    println!();

    let colony_config = load_colonyrc_with_trust().await;
    let agent_config = build_agent_config(&colony_config);

    let mut agent = match Agent::with_config(agent_config).await {
        Ok(agent) => {
            let (provider, model) = agent.current_provider();
            println!("🤖 Agent mode enabled");
            println!("   Provider: {:?}, Model: {}", provider, model);
            println!("   Use # or ? prefix for natural language queries");
            Some(agent)
        }
        Err(e) => {
            println!("⚠️  Agent mode disabled: {}", e);
            println!("   Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or install Ollama");
            None
        }
    };

    println!("   Type 'exit' to quit");

    let mut context = ExpansionContext::new();
    context.set_interactive(true);
    load_exported_functions(&mut context);
    init_secure_registry(&mut context);
    let mut executor = Executor::new(context);

    let observer = create_observer();
    executor.set_observer(observer.clone());
    if let Some(ref mut agent) = agent {
        agent.set_observer(observer.clone());
    }
    executor.channels_mut().set_observer(observer);

    println!("🔄 Reactive environment enabled");
    println!("   State changes emit tuples to tuple space\n");

    let cwd = std::env::current_dir().unwrap_or_default();
    let widget_server = match WidgetServer::new(cwd) {
        Ok(server) => {
            let socket_path = server.socket_path().to_string_lossy().to_string();
            std::env::set_var("COLONY_WIDGET_SOCKET", &socket_path);
            println!("🔌 Widget server: {}", socket_path);
            Some(server)
        }
        Err(e) => {
            tracing::warn!("Failed to start widget server: {}", e);
            None
        }
    };

    let shutdown_handle = widget_server.as_ref().map(|s| s.shutdown_handle());
    let cwd_handle = widget_server.as_ref().map(|s| s.cwd_handle());

    if let Some(server) = widget_server {
        tokio::spawn(async move {
            if let Err(e) = server.run().await {
                tracing::error!("Widget server error: {}", e);
            }
        });
    }

    #[cfg(feature = "tui")]
    let result = crate::repl_tui::run_repl_tui(agent, executor, cwd_handle).await;

    #[cfg(not(feature = "tui"))]
    let result = run_repl(agent, executor, cwd_handle).await;

    if let Some(shutdown) = shutdown_handle {
        let _ = shutdown.send(());
    }

    result
}
