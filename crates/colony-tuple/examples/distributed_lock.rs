//! Distributed lock demo
//!
//! This example demonstrates how persistent tuple space enables
//! distributed locks that work across multiple shell instances.
//!
//! Run multiple instances of this example concurrently:
//!
//! ```bash
//! # Terminal 1
//! cargo run --example distributed_lock -- worker-1
//!
//! # Terminal 2
//! cargo run --example distributed_lock -- worker-2
//!
//! # Terminal 3
//! cargo run --example distributed_lock -- worker-3
//! ```
//!
//! Only one worker can hold the lock at a time.

use colony_tuple::{PersistentTupleSpace, Tuple, Value};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let worker_id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "worker".to_string());

    println!("[{}] Starting worker", worker_id);

    // Open shared persistent tuple space
    let space = PersistentTupleSpace::open_default().await?;

    println!("[{}] Trying to acquire 'deploy' lock...", worker_id);

    // Try to acquire lock (non-blocking)
    let lock_pattern = Tuple::pattern(&[
        Value::Str("lock".to_string()),
        Value::Str("deploy".to_string()),
        Value::Wildcard,
    ]);

    // Check if lock exists
    if let Ok(Some(existing_lock)) = space.rdp(lock_pattern.clone()).await {
        let holder = match &existing_lock.fields[2] {
            Value::Str(s) => s.clone(),
            _ => "unknown".to_string(),
        };
        println!(
            "[{}] Lock is held by {}. Waiting...",
            worker_id, holder
        );

        // Wait for lock to be released
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            if space.rdp(lock_pattern.clone()).await?.is_none() {
                println!("[{}] Lock released, acquiring...", worker_id);
                break;
            }
        }
    }

    // Acquire lock
    space
        .out(Tuple::from(vec![
            Value::Str("lock".to_string()),
            Value::Str("deploy".to_string()),
            Value::Str(worker_id.clone()),
        ]))
        .await?;

    println!("[{}] ✓ Lock acquired!", worker_id);
    println!("[{}] Doing critical work...", worker_id);

    // Simulate work
    for i in 1..=5 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!("[{}] Working... {}/5", worker_id, i);
    }

    // Release lock
    space.inp(lock_pattern).await?;
    println!("[{}] ✓ Lock released", worker_id);

    Ok(())
}
