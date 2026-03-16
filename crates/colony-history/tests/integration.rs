use std::sync::Arc;

use colony_history::{
    HistoryRecord, HistoryStats, HistoryStore, HistoryWriter, PrivacyAction, PrivacyFilter,
    SearchQuery,
};
use tempfile::TempDir;

#[test]
fn test_full_pipeline() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let store = Arc::new(HistoryStore::open(&db_path).unwrap());

    // Start background writer
    let writer = HistoryWriter::start(store.clone());

    // Privacy filter
    let filter = PrivacyFilter::new(vec![]);

    // Record some commands
    let commands = vec![
        ("cargo build --release", "/project", 0, 5000u64),
        ("cargo test", "/project", 0, 12000),
        (" secret-command", "/project", 0, 100), // space-prefix -- skipped
        ("ls -la", "/home", 0, 10),
    ];

    for (cmd, cwd, exit_code, duration) in &commands {
        if filter.should_record(cmd) == PrivacyAction::Skip {
            continue;
        }
        let record = HistoryRecord::builder()
            .command(*cmd)
            .cwd(*cwd)
            .exit_code(*exit_code)
            .duration_ms(*duration)
            .session_id("test-session")
            .build()
            .unwrap();
        writer.send(record);
    }

    // Drop joins the thread -- all records flushed deterministically
    drop(writer);

    // Verify: 3 commands recorded (space-prefix skipped)
    assert_eq!(store.count().unwrap(), 3);

    // FTS search
    let results = store.search_fts("cargo", 50).unwrap();
    assert_eq!(results.len(), 2);

    // Advanced search with directory filter
    let query = SearchQuery::new("cargo").cwd_filter("/project").limit(10);
    let results = store.search_advanced(&query).unwrap();
    assert_eq!(results.len(), 2);

    // Stats
    let stats = HistoryStats::compute(&store).unwrap();
    assert_eq!(stats.total_commands, 3);
    assert!(stats.earliest_timestamp.is_some());
    assert!(stats.today_count >= 3);

    // Bookmark
    let first_result = store.search_fts("cargo build", 1).unwrap();
    assert_eq!(first_result.len(), 1);
    let id = &first_result[0].id;
    let bookmarked = store.toggle_bookmark(id).unwrap();
    assert!(bookmarked);

    // Note
    store.set_note(id, "important build").unwrap();

    // Tags
    store.set_tags(id, "build,release").unwrap();
}

#[test]
fn test_privacy_filter_integration() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let store = Arc::new(HistoryStore::open(&db_path).unwrap());

    let filter = PrivacyFilter::new(vec![regex::Regex::new(r"^export\s+.*SECRET").unwrap()]);

    let commands = vec![
        ("echo hello", true),
        (" hidden command", false),       // space prefix
        ("export API_SECRET=xyz", false), // regex exclusion
        ("ls -la", true),
    ];

    let writer = HistoryWriter::start(store.clone());
    for (cmd, should_record) in &commands {
        let action = filter.should_record(cmd);
        if *should_record {
            assert_eq!(action, PrivacyAction::Record, "expected Record for: {cmd}");
        } else {
            assert_eq!(action, PrivacyAction::Skip, "expected Skip for: {cmd}");
            continue;
        }
        let record = HistoryRecord::builder()
            .command(*cmd)
            .cwd("/tmp")
            .exit_code(0)
            .duration_ms(10)
            .session_id("test")
            .build()
            .unwrap();
        writer.send(record);
    }
    drop(writer);

    assert_eq!(store.count().unwrap(), 2);
}

#[test]
fn test_risk_scorer_integration() {
    use colony_history::{Executor, RiskLevel, RiskScorer};

    let scorer = RiskScorer::default();

    // Simulate an agent running commands
    let agent_commands = vec![
        ("rm -rf /tmp/build", RiskLevel::Critical),
        ("git push --force origin main", RiskLevel::High),
        ("git commit -m 'fix'", RiskLevel::Medium),
        ("cat README.md", RiskLevel::Low),
        ("echo done", RiskLevel::Safe),
    ];

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let store = Arc::new(HistoryStore::open(&db_path).unwrap());
    let writer = HistoryWriter::start(store.clone());

    for (cmd, expected_risk) in &agent_commands {
        let risk = scorer.score(cmd);
        assert_eq!(&risk, expected_risk, "wrong risk for: {cmd}");

        let record = HistoryRecord::builder()
            .command(*cmd)
            .cwd("/project")
            .exit_code(0)
            .duration_ms(100)
            .session_id("agent-session")
            .executor(Executor::External("claude-code".to_string()))
            .risk_level(risk)
            .build()
            .unwrap();
        writer.send(record);
    }
    drop(writer);

    assert_eq!(store.count().unwrap(), 5);

    // Agent summary should show claude-code
    let summary = store.agent_summary().unwrap();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].executor, "claude-code");
    assert_eq!(summary[0].total, 5);
}

#[test]
fn test_executor_detector_integration() {
    use colony_history::ExecutorDetector;
    use std::collections::HashMap;

    let detector = ExecutorDetector::default();

    let mut env = HashMap::new();
    let user = detector.detect(&env);
    assert_eq!(user, colony_history::Executor::User);

    env.insert("CLAUDE_CODE_ENTRYPOINT".to_string(), "1".to_string());
    let agent = detector.detect(&env);
    assert_eq!(
        agent,
        colony_history::Executor::External("claude-code".to_string())
    );
}

#[test]
fn test_export_integration() {
    use colony_history::ExportFormat;

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let store = HistoryStore::open(&db_path).unwrap();

    let record = HistoryRecord::builder()
        .command("echo hello")
        .cwd("/tmp")
        .exit_code(0)
        .duration_ms(5)
        .session_id("test")
        .build()
        .unwrap();
    store.insert(&record).unwrap();

    let json = store.export_all(ExportFormat::Json).unwrap();
    assert!(json.contains("echo hello"));

    let csv = store.export_all(ExportFormat::Csv).unwrap();
    assert!(csv.contains("echo hello"));

    let jsonl = store.export_all(ExportFormat::Jsonl).unwrap();
    assert!(jsonl.contains("echo hello"));
}
