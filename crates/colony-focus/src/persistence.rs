//! Goal persistence with SQLite
//!
//! Provides save/load operations for the goal tree, session restoration,
//! and schema migration support.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::error::{FocusError, Result};
use crate::goal::{Goal, GoalStatus};
use crate::tree::GoalTree;

const SCHEMA_VERSION: u32 = 1;

pub struct GoalStore {
    conn: Mutex<Connection>,
}

impl GoalStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.initialize_schema()?;
        store.run_migrations()?;
        Ok(store)
    }

    pub fn open_default() -> Result<Self> {
        let path = default_db_path();
        Self::open(path)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.initialize_schema()?;
        Ok(store)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| FocusError::Database(format!("lock poisoned: {e}")))
    }

    fn initialize_schema(&self) -> Result<()> {
        let conn = self.lock()?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS goals (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                parent_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (parent_id) REFERENCES goals(id)
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                name TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS session_goals (
                session_id TEXT NOT NULL,
                goal_id TEXT NOT NULL,
                PRIMARY KEY (session_id, goal_id),
                FOREIGN KEY (session_id) REFERENCES sessions(id),
                FOREIGN KEY (goal_id) REFERENCES goals(id)
            );

            CREATE INDEX IF NOT EXISTS idx_goals_parent
                ON goals(parent_id);
            CREATE INDEX IF NOT EXISTS idx_goals_status
                ON goals(status);
            CREATE INDEX IF NOT EXISTS idx_session_goals_session
                ON session_goals(session_id);",
        )?;

        let version: Option<u32> = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .ok();

        if version.is_none() {
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![SCHEMA_VERSION],
            )?;
        }

        Ok(())
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.lock()?;
        let version: u32 = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        if version < SCHEMA_VERSION {
            conn.execute(
                "UPDATE schema_version SET version = ?1",
                params![SCHEMA_VERSION],
            )?;
        }

        Ok(())
    }

    pub fn save_goal(&self, goal: &Goal) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR REPLACE INTO goals (id, title, description, status, parent_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                goal.id,
                goal.title,
                goal.description,
                goal.status.as_str(),
                goal.parent_id,
                goal.created_at,
                goal.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn save_tree(&self, tree: &GoalTree) -> Result<()> {
        let conn = self.lock()?;

        conn.execute_batch("PRAGMA foreign_keys = OFF")?;
        let tx = conn.unchecked_transaction()?;

        tx.execute("DELETE FROM session_goals", [])?;
        tx.execute("DELETE FROM goals", [])?;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO goals (id, title, description, status, parent_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;

            for goal in tree.all_goals() {
                stmt.execute(params![
                    goal.id,
                    goal.title,
                    goal.description,
                    goal.status.as_str(),
                    goal.parent_id,
                    goal.created_at,
                    goal.updated_at,
                ])?;
            }
        }

        tx.commit()?;
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        Ok(())
    }

    pub fn load_tree(&self) -> Result<GoalTree> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, description, status, parent_id, created_at, updated_at
             FROM goals",
        )?;

        let goals: Vec<Goal> = stmt
            .query_map([], |row| {
                let status_str: String = row.get(3)?;
                let children = Vec::new();

                Ok(Goal {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    status: GoalStatus::from_str(&status_str)
                        .unwrap_or(GoalStatus::Active),
                    parent_id: row.get(4)?,
                    children,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if goals.is_empty() {
            return Ok(GoalTree::new());
        }

        GoalTree::from_goals(goals)
    }

    pub fn get_goal(&self, id: &str) -> Result<Option<Goal>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, description, status, parent_id, created_at, updated_at
             FROM goals WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id], |row| {
                let status_str: String = row.get(3)?;
                Ok(Goal {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    status: GoalStatus::from_str(&status_str)
                        .unwrap_or(GoalStatus::Active),
                    parent_id: row.get(4)?,
                    children: Vec::new(),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .ok();

        Ok(result)
    }

    pub fn delete_goal(&self, id: &str) -> Result<bool> {
        let conn = self.lock()?;
        let count = conn.execute("DELETE FROM goals WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    pub fn create_session(&self, id: &str, name: Option<&str>) -> Result<()> {
        let conn = self.lock()?;
        let now = now_string();
        conn.execute(
            "INSERT INTO sessions (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, now, now],
        )?;
        Ok(())
    }

    pub fn link_goal_to_session(&self, session_id: &str, goal_id: &str) -> Result<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT OR IGNORE INTO session_goals (session_id, goal_id) VALUES (?1, ?2)",
            params![session_id, goal_id],
        )?;
        Ok(())
    }

    pub fn load_session_goals(&self, session_id: &str) -> Result<Vec<Goal>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT g.id, g.title, g.description, g.status, g.parent_id, g.created_at, g.updated_at
             FROM goals g
             JOIN session_goals sg ON sg.goal_id = g.id
             WHERE sg.session_id = ?1",
        )?;

        let goals = stmt
            .query_map(params![session_id], |row| {
                let status_str: String = row.get(3)?;
                Ok(Goal {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    status: GoalStatus::from_str(&status_str)
                        .unwrap_or(GoalStatus::Active),
                    parent_id: row.get(4)?,
                    children: Vec::new(),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(goals)
    }

    pub fn schema_version(&self) -> Result<u32> {
        let conn = self.lock()?;
        let version: u32 = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .map_err(|e| FocusError::Database(format!("failed to read schema version: {e}")))?;
        Ok(version)
    }

    pub fn goal_count(&self) -> Result<usize> {
        let conn = self.lock()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM goals", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

fn default_db_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("colony")
        .join("focus.db")
}

fn now_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + now.as_secs() / 31_536_000,
        (now.as_secs() % 31_536_000) / 2_592_000 + 1,
        (now.as_secs() % 2_592_000) / 86400 + 1,
        (now.as_secs() % 86400) / 3600,
        (now.as_secs() % 3600) / 60,
        now.as_secs() % 60,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> GoalStore {
        GoalStore::open_in_memory().unwrap()
    }

    fn make_goal(id: &str, title: &str) -> Goal {
        Goal::new(title).with_id(id)
    }

    #[test]
    fn test_open_in_memory() {
        let store = test_store();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn test_save_and_get_goal() {
        let store = test_store();
        let goal = make_goal("g-1", "Test goal");
        store.save_goal(&goal).unwrap();

        let loaded = store.get_goal("g-1").unwrap().unwrap();
        assert_eq!(loaded.id, "g-1");
        assert_eq!(loaded.title, "Test goal");
        assert_eq!(loaded.status, GoalStatus::Active);
    }

    #[test]
    fn test_save_goal_with_description() {
        let store = test_store();
        let goal = make_goal("g-1", "Test").with_description("A detailed description");
        store.save_goal(&goal).unwrap();

        let loaded = store.get_goal("g-1").unwrap().unwrap();
        assert_eq!(loaded.description.as_deref(), Some("A detailed description"));
    }

    #[test]
    fn test_save_goal_upsert() {
        let store = test_store();

        let goal = make_goal("g-1", "Original");
        store.save_goal(&goal).unwrap();

        let mut updated = make_goal("g-1", "Updated");
        updated.status = GoalStatus::Active;
        store.save_goal(&updated).unwrap();

        let loaded = store.get_goal("g-1").unwrap().unwrap();
        assert_eq!(loaded.title, "Updated");
    }

    #[test]
    fn test_get_goal_not_found() {
        let store = test_store();
        assert!(store.get_goal("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_delete_goal() {
        let store = test_store();
        store.save_goal(&make_goal("g-1", "Test")).unwrap();

        assert!(store.delete_goal("g-1").unwrap());
        assert!(store.get_goal("g-1").unwrap().is_none());
    }

    #[test]
    fn test_delete_goal_not_found() {
        let store = test_store();
        assert!(!store.delete_goal("nonexistent").unwrap());
    }

    #[test]
    fn test_save_and_load_tree() {
        let store = test_store();
        let mut tree = GoalTree::new();
        tree.add_root(make_goal("g-1", "Root")).unwrap();
        tree.add_child("g-1", make_goal("g-2", "Child A")).unwrap();
        tree.add_child("g-1", make_goal("g-3", "Child B")).unwrap();
        tree.add_child("g-2", make_goal("g-4", "Grandchild")).unwrap();

        store.save_tree(&tree).unwrap();

        let loaded = store.load_tree().unwrap();
        assert_eq!(loaded.len(), 4);
        assert_eq!(loaded.roots().len(), 1);
        assert_eq!(loaded.roots()[0].id, "g-1");
        assert_eq!(loaded.get_children("g-1").len(), 2);
        assert_eq!(loaded.get_children("g-2").len(), 1);
    }

    #[test]
    fn test_load_empty_tree() {
        let store = test_store();
        let tree = store.load_tree().unwrap();
        assert!(tree.is_empty());
    }

    #[test]
    fn test_save_tree_replaces_all() {
        let store = test_store();

        let mut tree1 = GoalTree::new();
        tree1.add_root(make_goal("g-1", "First")).unwrap();
        store.save_tree(&tree1).unwrap();
        assert_eq!(store.goal_count().unwrap(), 1);

        let mut tree2 = GoalTree::new();
        tree2.add_root(make_goal("g-2", "Second")).unwrap();
        tree2.add_root(make_goal("g-3", "Third")).unwrap();
        store.save_tree(&tree2).unwrap();

        assert_eq!(store.goal_count().unwrap(), 2);
        assert!(store.get_goal("g-1").unwrap().is_none());
        assert!(store.get_goal("g-2").unwrap().is_some());
    }

    #[test]
    fn test_goal_with_status_persistence() {
        let store = test_store();
        let mut goal = make_goal("g-1", "Test");
        goal.transition(GoalStatus::Blocked).unwrap();
        store.save_goal(&goal).unwrap();

        let loaded = store.get_goal("g-1").unwrap().unwrap();
        assert_eq!(loaded.status, GoalStatus::Blocked);
    }

    #[test]
    fn test_session_goals() {
        let store = test_store();

        store.save_goal(&make_goal("g-1", "Goal A")).unwrap();
        store.save_goal(&make_goal("g-2", "Goal B")).unwrap();

        store.create_session("session-1", Some("Test session")).unwrap();
        store.link_goal_to_session("session-1", "g-1").unwrap();
        store.link_goal_to_session("session-1", "g-2").unwrap();

        let goals = store.load_session_goals("session-1").unwrap();
        assert_eq!(goals.len(), 2);
    }

    #[test]
    fn test_session_goals_empty() {
        let store = test_store();
        store.create_session("session-1", None).unwrap();

        let goals = store.load_session_goals("session-1").unwrap();
        assert!(goals.is_empty());
    }

    #[test]
    fn test_goal_count() {
        let store = test_store();
        assert_eq!(store.goal_count().unwrap(), 0);

        store.save_goal(&make_goal("g-1", "One")).unwrap();
        store.save_goal(&make_goal("g-2", "Two")).unwrap();
        assert_eq!(store.goal_count().unwrap(), 2);
    }

    #[test]
    fn test_open_at_path() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("focus.db");

        let store = GoalStore::open(&db_path).unwrap();
        store.save_goal(&make_goal("g-1", "Test")).unwrap();
        drop(store);

        let store2 = GoalStore::open(&db_path).unwrap();
        let loaded = store2.get_goal("g-1").unwrap().unwrap();
        assert_eq!(loaded.title, "Test");
    }

    #[test]
    fn test_parent_child_persistence() {
        let store = test_store();

        let root = make_goal("g-1", "Root");
        let mut child = make_goal("g-2", "Child");
        child.parent_id = Some("g-1".to_string());

        store.save_goal(&root).unwrap();
        store.save_goal(&child).unwrap();

        let loaded = store.get_goal("g-2").unwrap().unwrap();
        assert_eq!(loaded.parent_id.as_deref(), Some("g-1"));
    }

    #[test]
    fn test_duplicate_session_link() {
        let store = test_store();
        store.save_goal(&make_goal("g-1", "Goal")).unwrap();
        store.create_session("s-1", None).unwrap();
        store.link_goal_to_session("s-1", "g-1").unwrap();
        store.link_goal_to_session("s-1", "g-1").unwrap();

        let goals = store.load_session_goals("s-1").unwrap();
        assert_eq!(goals.len(), 1);
    }
}
