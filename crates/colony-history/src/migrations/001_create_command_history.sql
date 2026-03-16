CREATE TABLE IF NOT EXISTS command_history (
    id TEXT PRIMARY KEY,
    command TEXT NOT NULL,
    command_redacted TEXT NOT NULL,
    cwd TEXT NOT NULL,
    exit_code INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    executor TEXT NOT NULL DEFAULT 'user',
    agent_session_id TEXT,
    agent_goal TEXT,
    risk_level TEXT,
    session_id TEXT NOT NULL,
    git_branch TEXT,
    hostname TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    hidden INTEGER NOT NULL DEFAULT 0,
    bookmarked INTEGER NOT NULL DEFAULT 0,
    note TEXT,
    tags TEXT
);

CREATE INDEX IF NOT EXISTS idx_history_timestamp ON command_history(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_history_command ON command_history(command);
CREATE INDEX IF NOT EXISTS idx_history_cwd ON command_history(cwd);
CREATE INDEX IF NOT EXISTS idx_history_executor ON command_history(executor);
CREATE INDEX IF NOT EXISTS idx_history_session ON command_history(session_id);

CREATE VIRTUAL TABLE IF NOT EXISTS command_history_fts USING fts5(
    command,
    content=command_history,
    content_rowid=rowid
);

-- Triggers to keep FTS index in sync with content table
CREATE TRIGGER IF NOT EXISTS command_history_ai AFTER INSERT ON command_history BEGIN
    INSERT INTO command_history_fts(rowid, command) VALUES (new.rowid, new.command);
END;

CREATE TRIGGER IF NOT EXISTS command_history_ad AFTER DELETE ON command_history BEGIN
    INSERT INTO command_history_fts(command_history_fts, rowid, command) VALUES('delete', old.rowid, old.command);
END;

CREATE TRIGGER IF NOT EXISTS command_history_au AFTER UPDATE ON command_history BEGIN
    INSERT INTO command_history_fts(command_history_fts, rowid, command) VALUES('delete', old.rowid, old.command);
    INSERT INTO command_history_fts(rowid, command) VALUES (new.rowid, new.command);
END;

-- Migration tracking
CREATE TABLE IF NOT EXISTS _colony_history_migrations (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at INTEGER NOT NULL
);
