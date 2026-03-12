use crate::{default_approvals_db_path, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Actor string used when the shell user manually grants an approval through
/// an interactive prompt (as opposed to a programmatic or scripted grant).
pub const GRANTED_BY_INTERACTIVE: &str = "interactive";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalScope {
    Global,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureApproval {
    pub script_path: PathBuf,
    pub script_hash: String,
    pub variable_name: String,
    pub scope: ApprovalScope,
    pub granted_at: DateTime<Utc>,
    pub granted_by: String,
}

pub struct ApprovalDatabase {
    conn: Connection,
}

impl ApprovalDatabase {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn open_default() -> Result<Self> {
        Self::open(&default_approvals_db_path())
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS approvals (
                id INTEGER PRIMARY KEY,
                script_path TEXT NOT NULL,
                script_hash TEXT NOT NULL,
                variable_name TEXT NOT NULL,
                scope TEXT NOT NULL DEFAULT 'global',
                granted_at TEXT NOT NULL,
                granted_by TEXT NOT NULL,
                UNIQUE(script_path, script_hash, variable_name)
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_script ON approvals(script_path)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_variable ON approvals(variable_name)",
            [],
        )?;

        Ok(())
    }

    /// Check if an approval exists for the given script, hash, and variable.
    ///
    /// Scope is **not** filtered: a matching approval grants access regardless
    /// of whether it was saved as `Global` or `Project`. Scope is recorded as
    /// metadata for auditing, not as a check-time predicate.
    pub fn check_approval(
        &self,
        script_path: &Path,
        script_hash: &str,
        variable: &str,
    ) -> Result<bool> {
        let script_path_str = script_path.to_string_lossy();

        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM approvals 
             WHERE script_path = ?1 AND script_hash = ?2 AND variable_name = ?3",
            params![script_path_str, script_hash, variable],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    pub fn save_approval(
        &self,
        script_path: &Path,
        script_hash: &str,
        variable: &str,
        granted_by: &str,
        scope: ApprovalScope,
    ) -> Result<()> {
        let script_path_str = script_path.to_string_lossy();
        let granted_at = Utc::now().to_rfc3339();
        let scope_str = match scope {
            ApprovalScope::Global => "global",
            ApprovalScope::Project => "project",
        };

        self.conn.execute(
            "INSERT OR REPLACE INTO approvals
             (script_path, script_hash, variable_name, scope, granted_at, granted_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                script_path_str,
                script_hash,
                variable,
                scope_str,
                granted_at,
                granted_by
            ],
        )?;

        Ok(())
    }

    pub fn revoke_approval(&self, script_path: &Path, variable: &str) -> Result<()> {
        let script_path_str = script_path.to_string_lossy();

        self.conn.execute(
            "DELETE FROM approvals WHERE script_path = ?1 AND variable_name = ?2",
            params![script_path_str, variable],
        )?;

        Ok(())
    }

    pub fn revoke_all_approvals(&self, variable: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM approvals WHERE variable_name = ?1",
            params![variable],
        )?;

        Ok(())
    }

    pub fn list_approvals(&self, variable: &str) -> Result<Vec<SecureApproval>> {
        let mut stmt = self.conn.prepare(
            "SELECT script_path, script_hash, variable_name, scope, granted_at, granted_by
             FROM approvals WHERE variable_name = ?1",
        )?;

        let approvals = stmt.query_map(params![variable], |row| {
            let script_path: String = row.get(0)?;
            let script_hash: String = row.get(1)?;
            let variable_name: String = row.get(2)?;
            let scope_str: String = row.get(3)?;
            let granted_at_str: String = row.get(4)?;
            let granted_by: String = row.get(5)?;

            let scope = if scope_str == "project" {
                ApprovalScope::Project
            } else {
                ApprovalScope::Global
            };

            let granted_at = DateTime::parse_from_rfc3339(&granted_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            Ok(SecureApproval {
                script_path: PathBuf::from(script_path),
                script_hash,
                variable_name,
                scope,
                granted_at,
                granted_by,
            })
        })?;

        let mut result = Vec::new();
        for approval in approvals {
            result.push(approval?);
        }

        Ok(result)
    }

    pub fn list_all_approvals(&self) -> Result<Vec<SecureApproval>> {
        let mut stmt = self.conn.prepare(
            "SELECT script_path, script_hash, variable_name, scope, granted_at, granted_by
             FROM approvals",
        )?;

        let approvals = stmt.query_map([], |row| {
            let script_path: String = row.get(0)?;
            let script_hash: String = row.get(1)?;
            let variable_name: String = row.get(2)?;
            let scope_str: String = row.get(3)?;
            let granted_at_str: String = row.get(4)?;
            let granted_by: String = row.get(5)?;

            let scope = if scope_str == "project" {
                ApprovalScope::Project
            } else {
                ApprovalScope::Global
            };

            let granted_at = DateTime::parse_from_rfc3339(&granted_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            Ok(SecureApproval {
                script_path: PathBuf::from(script_path),
                script_hash,
                variable_name,
                scope,
                granted_at,
                granted_by,
            })
        })?;

        let mut result = Vec::new();
        for approval in approvals {
            result.push(approval?);
        }

        Ok(result)
    }

    pub fn clear_stale_approvals(&self, script_path: &Path, current_hash: &str) -> Result<usize> {
        let script_path_str = script_path.to_string_lossy();

        let deleted = self.conn.execute(
            "DELETE FROM approvals WHERE script_path = ?1 AND script_hash != ?2",
            params![script_path_str, current_hash],
        )?;

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_database_operations() {
        let db = ApprovalDatabase::open_in_memory().unwrap();
        let script_path = PathBuf::from("/home/user/deploy.sh");
        let script_hash = "sha256:abc123";
        let variable = "AWS_SECRET_KEY";

        assert!(!db
            .check_approval(&script_path, script_hash, variable)
            .unwrap());

        db.save_approval(
            &script_path,
            script_hash,
            variable,
            GRANTED_BY_INTERACTIVE,
            ApprovalScope::Global,
        )
        .unwrap();
        assert!(db
            .check_approval(&script_path, script_hash, variable)
            .unwrap());

        let wrong_hash = "sha256:different";
        assert!(!db
            .check_approval(&script_path, wrong_hash, variable)
            .unwrap());

        let approvals = db.list_approvals(variable).unwrap();
        assert_eq!(approvals.len(), 1);
        assert_eq!(approvals[0].script_path, script_path);
        assert_eq!(approvals[0].granted_by, GRANTED_BY_INTERACTIVE);

        db.revoke_approval(&script_path, variable).unwrap();
        assert!(!db
            .check_approval(&script_path, script_hash, variable)
            .unwrap());
    }

    #[test]
    fn test_revoke_all_approvals() {
        let db = ApprovalDatabase::open_in_memory().unwrap();
        let variable = "SHARED_SECRET";

        db.save_approval(
            &PathBuf::from("/script1.sh"),
            "sha256:hash1",
            variable,
            "user1",
            ApprovalScope::Global,
        )
        .unwrap();
        db.save_approval(
            &PathBuf::from("/script2.sh"),
            "sha256:hash2",
            variable,
            "user2",
            ApprovalScope::Global,
        )
        .unwrap();

        let approvals = db.list_approvals(variable).unwrap();
        assert_eq!(approvals.len(), 2);

        db.revoke_all_approvals(variable).unwrap();

        let approvals = db.list_approvals(variable).unwrap();
        assert_eq!(approvals.len(), 0);
    }

    #[test]
    fn test_clear_stale_approvals() {
        let db = ApprovalDatabase::open_in_memory().unwrap();
        let script_path = PathBuf::from("/deploy.sh");
        let variable = "SECRET";

        db.save_approval(&script_path, "sha256:old", variable, "user", ApprovalScope::Global)
            .unwrap();
        db.save_approval(&script_path, "sha256:new", variable, "user", ApprovalScope::Global)
            .unwrap();

        let cleared = db
            .clear_stale_approvals(&script_path, "sha256:new")
            .unwrap();
        assert_eq!(cleared, 1);

        assert!(db
            .check_approval(&script_path, "sha256:new", variable)
            .unwrap());
        assert!(!db
            .check_approval(&script_path, "sha256:old", variable)
            .unwrap());
    }

    #[test]
    fn test_scope_is_persisted() {
        let db = ApprovalDatabase::open_in_memory().unwrap();
        let global_path = PathBuf::from("/home/user/global-script.sh");
        let project_path = PathBuf::from("/home/user/project-script.sh");
        let variable = "DB_PASSWORD";

        db.save_approval(&global_path, "sha256:g1", variable, "user", ApprovalScope::Global)
            .unwrap();
        db.save_approval(
            &project_path,
            "sha256:p1",
            variable,
            "user",
            ApprovalScope::Project,
        )
        .unwrap();

        let approvals = db.list_approvals(variable).unwrap();
        assert_eq!(approvals.len(), 2);

        let global_approval = approvals
            .iter()
            .find(|a| a.script_path == global_path)
            .expect("global approval should be present");
        assert_eq!(global_approval.scope, ApprovalScope::Global);

        let project_approval = approvals
            .iter()
            .find(|a| a.script_path == project_path)
            .expect("project approval should be present");
        assert_eq!(project_approval.scope, ApprovalScope::Project);
    }
}
