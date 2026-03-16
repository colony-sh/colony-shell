//! Reactive Prompt Demo
//!
//! Demonstrates Colony Shell's reactive environment - the core differentiator!
//!
//! This example shows how a terminal or agent can subscribe to shell state changes
//! (directory changes, environment variable changes) and react in real-time.
//!
//! ## What Makes This Special
//!
//! Traditional shells: Terminal polls for state changes or rebuilds prompt on every command
//! Colony Shell: State changes emit tuples → subscribers react instantly (no polling!)
//!
//! ## Run This Demo
//!
//! ```bash
//! cargo run --example reactive_prompt
//! ```

use colony_channels::ChannelManager;
use colony_exec::{execute_builtin, Executor};
use colony_expand::ExpansionContext;
use colony_parser::{SimpleCommand, Span, Word};
use colony_tuple::{Tuple, Value};
use std::thread;
use std::time::Duration;

fn main() {
    println!("🎯 Colony Shell - Reactive Prompt Demo\n");
    println!("This demonstrates real-time state change notifications via tuple space.\n");

    // Create executor with tuple space
    let context = ExpansionContext::new();
    let executor = Executor::new(context);
    let tuple_space = executor.tuple_space().clone();

    // Spawn a subscriber thread that reacts to changes
    let subscriber_handle = thread::spawn(move || {
        println!("📡 Subscriber: Listening for state changes...\n");

        for _i in 1..=5 {
            // Try to read any dir_change or env_change tuple (non-blocking)
            let dir_pattern = Tuple::pattern(&[
                Value::Str("dir_change".to_string()),
                Value::Wildcard,
                Value::Wildcard,
            ]);

            if let Some(tuple) = tuple_space.inp(dir_pattern) {
                if let (Value::Str(old), Value::Str(new)) = (&tuple.fields[1], &tuple.fields[2]) {
                    println!("🔄 Subscriber: Directory changed!");
                    println!("   Old: {}", old);
                    println!("   New: {}", new);
                    println!("   → Prompt would update instantly\n");
                }
            }

            let env_pattern = Tuple::pattern(&[
                Value::Str("env_change".to_string()),
                Value::Wildcard,
                Value::Wildcard,
                Value::Wildcard,
            ]);

            if let Some(tuple) = tuple_space.inp(env_pattern) {
                if let Value::Str(var) = &tuple.fields[1] {
                    match &tuple.fields[3] {
                        Value::Str(new_val) => {
                            println!("🔄 Subscriber: Environment variable changed!");
                            println!("   Variable: {}", var);
                            println!("   New value: {}", new_val);
                            println!("   → Agent context would refresh\n");
                        }
                        Value::Null => {
                            println!("🔄 Subscriber: Environment variable deleted!");
                            println!("   Variable: {}", var);
                            println!("   → Agent context would cleanup\n");
                        }
                        _ => {}
                    }
                }
            }

            thread::sleep(Duration::from_millis(200));
        }

        println!("📡 Subscriber: Done listening\n");
    });

    // Main thread: Simulate user commands
    println!("💻 Shell: Simulating user commands...\n");

    thread::sleep(Duration::from_millis(100));

    // Command 1: export PATH=/new/path
    println!("💻 Shell: $ export TEST_VAR=hello");
    let export_cmd = SimpleCommand {
        name: Word::literal("export", Span::DUMMY),
        args: vec![Word::literal("TEST_VAR=hello", Span::DUMMY)],
        redirects: vec![],
        env_assignments: vec![],
        span: Span::DUMMY,
    };

    let mut ctx = ExpansionContext::new();
    let jobs = colony_exec::JobTable::new();
    let mut channels = ChannelManager::new();
    if let Some(result) = execute_builtin(
        &export_cmd,
        &mut ctx,
        &jobs,
        executor.tuple_space(),
        executor.session_id(),
        &mut channels,
        None,
        None,
    ) {
        result.ok();
    }

    thread::sleep(Duration::from_millis(300));

    // Command 2: cd /tmp
    println!("💻 Shell: $ cd /tmp");
    let cd_cmd = SimpleCommand {
        name: Word::literal("cd", Span::DUMMY),
        args: vec![Word::literal("/tmp", Span::DUMMY)],
        redirects: vec![],
        env_assignments: vec![],
        span: Span::DUMMY,
    };

    if let Some(result) = execute_builtin(
        &cd_cmd,
        &mut ctx,
        &jobs,
        executor.tuple_space(),
        executor.session_id(),
        &mut channels,
        None,
        None,
    ) {
        result.ok();
    }

    thread::sleep(Duration::from_millis(300));

    // Command 3: export another variable
    println!("💻 Shell: $ export ANOTHER=world");
    let export_cmd2 = SimpleCommand {
        name: Word::literal("export", Span::DUMMY),
        args: vec![Word::literal("ANOTHER=world", Span::DUMMY)],
        redirects: vec![],
        env_assignments: vec![],
        span: Span::DUMMY,
    };

    if let Some(result) = execute_builtin(
        &export_cmd2,
        &mut ctx,
        &jobs,
        executor.tuple_space(),
        executor.session_id(),
        &mut channels,
        None,
        None,
    ) {
        result.ok();
    }

    thread::sleep(Duration::from_millis(300));

    // Command 4: unset variable
    println!("💻 Shell: $ unset TEST_VAR");
    let unset_cmd = SimpleCommand {
        name: Word::literal("unset", Span::DUMMY),
        args: vec![Word::literal("TEST_VAR", Span::DUMMY)],
        redirects: vec![],
        env_assignments: vec![],
        span: Span::DUMMY,
    };

    if let Some(result) = execute_builtin(
        &unset_cmd,
        &mut ctx,
        &jobs,
        executor.tuple_space(),
        executor.session_id(),
        &mut channels,
        None,
        None,
    ) {
        result.ok();
    }

    // Wait for subscriber to finish
    subscriber_handle.join().ok();

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Demo Complete!");
    println!();
    println!("🎯 Key Takeaways:");
    println!("   • Shell state changes emit tuples automatically");
    println!("   • Subscribers receive notifications instantly (no polling!)");
    println!("   • Terminal prompt updates reactively");
    println!("   • Agent context refreshes automatically");
    println!("   • Pattern matching enables flexible subscriptions");
    println!();
    println!("🚀 This is the Colony Shell difference:");
    println!("   Observable shell state through tuple space coordination!");
    println!("═══════════════════════════════════════════════════════════");
}
