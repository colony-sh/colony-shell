//! IPC tuple space demonstration
//!
//! Shows how IpcTupleSpace provides the same API as PersistentTupleSpace
//! but with infrastructure for event-driven notifications.
//!
//! Run:
//! ```bash
//! cargo run --example ipc_demo
//! ```

use colony_tuple::{IpcTupleSpace, Tuple, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔗 IPC Tuple Space Demo\n");

    let space = IpcTupleSpace::open_default().await?;

    // Clear for clean demo
    space.clear().await?;

    println!("Writing tuples to IPC-enabled space...");

    // Write some tuples
    space
        .out(Tuple::from(vec![
            Value::Str("event".to_string()),
            Value::Str("build_complete".to_string()),
            Value::Int(42),
        ]))
        .await?;

    space
        .out(Tuple::from(vec![
            Value::Str("lock".to_string()),
            Value::Str("deploy".to_string()),
            Value::Str("session-123".to_string()),
        ]))
        .await?;

    println!("✓ Wrote 2 tuples");
    println!("✓ Tuples persisted to: ~/.config/colony/tuple-space.db");
    println!("✓ IPC notifications sent (when fully implemented)\n");

    println!("Querying tuples...");

    // Query by pattern
    let pattern = Tuple::pattern(&[Value::Str("event".to_string()), Value::Wildcard, Value::Wildcard]);

    if let Some(event) = space.rdp(pattern).await? {
        println!("Found event tuple: {:?}", event.fields);
    }

    let pattern = Tuple::pattern(&[Value::Str("lock".to_string()), Value::Wildcard, Value::Wildcard]);

    if let Some(lock) = space.rdp(pattern.clone()).await? {
        println!("Found lock tuple: {:?}", lock.fields);

        // Release the lock
        println!("\nReleasing lock...");
        space.inp(pattern).await?;
        println!("✓ Lock released");
    }

    println!("\nTuples remaining: {}", space.len().await);

    println!("\n🎯 Key Features:");
    println!("  - Persistent storage (survives shell restarts)");
    println!("  - Event-driven notifications (instant coordination)");
    println!("  - Compatible with PersistentTupleSpace API");
    println!("  - Foundation for shell ↔ terminal coordination");

    Ok(())
}
