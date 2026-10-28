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
use colony_history::{HistoryStore, HistoryWriter};
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

    // Debug/X-Ray mode (F071)
    if opts.debug || opts.xray {
        let mode = if opts.debug {
            colony_debug::DebugMode::Debug
        } else {
            colony_debug::DebugMode::XRay
        };

        let source = match std::fs::read_to_string(script) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("colony: {}: {}", script.display(), e);
                std::process::exit(1);
            }
        };

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let controller = colony_debug::DebugController::new(mode, event_tx, cmd_rx);
        let controller = std::sync::Arc::new(tokio::sync::Mutex::new(controller));

        let mut context = ExpansionContext::new();
        load_exported_functions(&mut context);
        init_secure_registry(&mut context);
        context.set_shell_name(script.display().to_string());
        context.set_current_script_path(Some(script.to_path_buf()));
        context.set_positional_params(opts.positional_args.clone());

        let mut executor = Executor::new(context);
        executor.set_observer(create_observer());
        executor.set_debug_controller(controller);
        run_startup_files(opts, &mut executor).await;

        // The executor is not Send (recursive futures use Pin<Box<dyn Future>>),
        // so we run both futures on the current task using tokio::join!
        // The TUI runs in a blocking thread since it needs crossterm raw mode.
        let script_path = script.to_path_buf();
        let tui_source = source.clone();

        // Spawn TUI on a blocking thread (needs terminal control)
        let tui_handle = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                if let Err(e) = colony_debug::tui::run(&tui_source, mode, event_rx, cmd_tx).await {
                    eprintln!("colony: debug TUI error: {}", e);
                }
            });
        });

        let exec_future = async {
            match executor.execute_source_file(&script_path, vec![]).await {
                Ok(status) => status.code(),
                Err(e) => {
                    eprintln!("colony: {}: {}", script_path.display(), e);
                    127
                }
            }
        };

        // Run executor future. The TUI runs concurrently on a blocking thread.
        // We use select! only for ctrl+c handling; exec_future drives the executor.
        pin!(exec_future);

        let code = select! {
            _ = signal::ctrl_c() => {
                eprintln!("colony: interrupted (SIGINT)");
                130
            }
            code = &mut exec_future => code,
        };

        // Abort TUI blocking task (it will exit when channels close)
        tui_handle.abort();
        std::process::exit(code);
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
    // First-run onboarding wizard (runs before REPL/TUI initialization)
    if crate::onboarding::check_first_run() {
        crate::onboarding::wizard::run();
    }

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

    // Initialise the SQLite-backed history store and background writer
    let (history_store, history_writer) = match HistoryStore::open(&HistoryStore::default_path()) {
        Ok(store) => {
            let store = Arc::new(store);
            let writer = HistoryWriter::start(store.clone());
            tracing::info!("History store opened: {:?}", store.db_path());
            (Some(store), Some(writer))
        }
        Err(e) => {
            tracing::warn!("Failed to open history store: {}", e);
            (None, None)
        }
    };

    let mut context = ExpansionContext::new();
    context.set_interactive(true);
    load_exported_functions(&mut context);
    init_secure_registry(&mut context);
    let mut executor = Executor::new(context);

    if let Some(ref store) = history_store {
        executor.set_history_store(store.clone());
    }

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
    let result = {
        // History writer and store are dropped when this scope ends,
        // flushing any remaining records to disk.
        let _writer = history_writer;
        let _store = history_store;
        crate::repl_tui::run_repl_tui(agent, executor, cwd_handle).await
    };

    #[cfg(not(feature = "tui"))]
    let result = run_repl(agent, executor, cwd_handle, history_writer, history_store).await;

    if let Some(shutdown) = shutdown_handle {
        let _ = shutdown.send(());
    }

    result
}
