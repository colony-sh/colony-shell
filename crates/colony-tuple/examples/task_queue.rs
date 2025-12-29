//! Persistent task queue demo
//!
//! This example demonstrates how persistent tuple space enables
//! task queues that survive shell restarts.
//!
//! Producer: Adds tasks to the queue
//! ```bash
//! cargo run --example task_queue producer
//! ```
//!
//! Worker: Processes tasks from the queue
//! ```bash
//! cargo run --example task_queue worker
//! ```
//!
//! You can:
//! - Start producer, add tasks, kill it
//! - Start worker later - tasks are still there!
//! - Run multiple workers - they share the queue

use colony_tuple::{PersistentTupleSpace, Tuple, Value};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| {
        println!("Usage: task_queue <producer|worker>");
        std::process::exit(1);
    });

    let space = PersistentTupleSpace::open_default().await?;

    match mode.as_str() {
        "producer" => run_producer(space).await,
        "worker" => run_worker(space).await,
        _ => {
            println!("Unknown mode: {}. Use 'producer' or 'worker'", mode);
            std::process::exit(1);
        }
    }
}

async fn run_producer(space: PersistentTupleSpace) -> Result<(), Box<dyn std::error::Error>> {
    println!("🏭 Task Producer");
    println!("Adding 5 tasks to the persistent queue...\n");

    let tasks = [
        ("build", "Build the project"),
        ("test", "Run test suite"),
        ("lint", "Check code style"),
        ("docs", "Generate documentation"),
        ("deploy", "Deploy to production"),
    ];

    for (i, (task_type, description)) in tasks.iter().enumerate() {
        let task_id = format!("task-{}", i + 1);

        space
            .out(Tuple::from(vec![
                Value::Str("task".to_string()),
                Value::Str(task_id.clone()),
                Value::Str(task_type.to_string()),
                Value::Str(description.to_string()),
            ]))
            .await?;

        println!("✓ Added task: {} ({})", task_id, description);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let count = space.len().await;
    println!("\n✓ Done! {} tasks in queue", count);
    println!("Tasks will persist even if this process exits.");
    println!("Start a worker to process them: cargo run --example task_queue worker");

    Ok(())
}

async fn run_worker(space: PersistentTupleSpace) -> Result<(), Box<dyn std::error::Error>> {
    println!("⚙️  Task Worker");
    println!("Checking for tasks...\n");

    let initial_count = space.len().await;
    println!("Found {} tasks in queue", initial_count);

    if initial_count == 0 {
        println!("Queue is empty. Add tasks with: cargo run --example task_queue producer");
        return Ok(());
    }

    println!("Processing tasks...\n");

    // Process tasks from queue
    loop {
        let pattern = Tuple::pattern(&[
            Value::Str("task".to_string()),
            Value::Wildcard, // Any task ID
            Value::Wildcard, // Any task type
            Value::Wildcard, // Any description
        ]);

        // Try to take a task (non-blocking)
        match space.inp(pattern).await? {
            Some(tuple) => {
                let task_id = match &tuple.fields[1] {
                    Value::Str(s) => s.clone(),
                    _ => "unknown".to_string(),
                };
                let task_type = match &tuple.fields[2] {
                    Value::Str(s) => s.clone(),
                    _ => "unknown".to_string(),
                };
                let description = match &tuple.fields[3] {
                    Value::Str(s) => s.clone(),
                    _ => "unknown".to_string(),
                };

                println!("📋 Processing: {} - {}", task_id, description);

                // Simulate work
                tokio::time::sleep(Duration::from_millis(1000)).await;

                println!("✓ Completed: {} ({})\n", task_id, task_type);
            }
            None => {
                println!("✓ All tasks processed!");
                break;
            }
        }
    }

    println!("Worker finished. Queue is now empty.");

    Ok(())
}
