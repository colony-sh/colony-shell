//! Integration test: full observation lifecycle

use colony_reflection::{
    AgentChatData, CommandData, ContextUsageData, ErrorData, FileActivityData, NoopObserver,
    ObservationCategory, ObservationStore, ReflectionObserver, SqliteObserver,
};

#[tokio::test]
async fn test_full_observation_lifecycle() {
    let store = ObservationStore::open_in_memory().unwrap();
    let observer = SqliteObserver::with_store(store, "integration-test".to_string());

    // Simulate a realistic session
    observer
        .observe_command(CommandData {
            input: "cd project/".to_string(),
            exit_code: 0,
            duration_ms: 1,
            pipeline_length: 1,
        })
        .await;

    observer
        .observe_command(CommandData {
            input: "cargo build".to_string(),
            exit_code: 0,
            duration_ms: 15000,
            pipeline_length: 1,
        })
        .await;

    observer
        .observe_error(ErrorData {
            kind: "CommandNotFound".to_string(),
            message: "typo_command not found".to_string(),
            recoverable: true,
        })
        .await;

    observer
        .observe_file_activity(FileActivityData {
            path: "src/main.rs".to_string(),
            action: "modified".to_string(),
            language: Some("rust".to_string()),
        })
        .await;

    observer
        .observe_agent_chat(AgentChatData {
            provider: "anthropic".to_string(),
            model: "claude-sonnet".to_string(),
            prompt_tokens: 1500,
            completion_tokens: 500,
            duration_ms: 2000,
        })
        .await;

    observer
        .observe_context_usage(ContextUsageData {
            channel: "fd3".to_string(),
            bytes: 800,
            direction: "write".to_string(),
        })
        .await;

    // Verify counts
    let store = observer.store();
    assert_eq!(store.count(None).unwrap(), 6);
    assert_eq!(store.count(Some(ObservationCategory::Command)).unwrap(), 2);
    assert_eq!(store.count(Some(ObservationCategory::Error)).unwrap(), 1);
    assert_eq!(
        store.count(Some(ObservationCategory::AgentChat)).unwrap(),
        1
    );
    assert_eq!(
        store
            .count(Some(ObservationCategory::FileActivity))
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .count(Some(ObservationCategory::ContextUsage))
            .unwrap(),
        1
    );

    // Verify session query
    let all = store.by_session("integration-test").unwrap();
    assert_eq!(all.len(), 6);

    // Verify command counts increment
    assert_eq!(all[0].context.command_count, 1);
    assert_eq!(all[1].context.command_count, 2);

    // Verify timestamps are ordered
    for window in all.windows(2) {
        assert!(window[0].timestamp <= window[1].timestamp);
    }
}

#[tokio::test]
async fn test_noop_observer_is_zero_cost() {
    let observer = NoopObserver;
    for _ in 0..1000 {
        observer
            .observe_command(CommandData {
                input: "ls".to_string(),
                exit_code: 0,
                duration_ms: 1,
                pipeline_length: 1,
            })
            .await;
    }
}
