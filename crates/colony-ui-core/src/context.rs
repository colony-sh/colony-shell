//! Component context - data passed to components during render

use std::collections::HashMap;
use std::path::PathBuf;

/// Context passed to components during render
///
/// This provides all data components might need without reaching into global state.
#[derive(Debug, Clone)]
pub struct ComponentContext {
    /// Current working directory
    pub cwd: PathBuf,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Last command exit code
    pub last_exit_code: i32,
    /// Number of running background jobs
    pub running_jobs: usize,
    /// Number of stopped jobs
    pub stopped_jobs: usize,
    /// Current AI model (if agent active)
    pub model: Option<String>,
}

impl ComponentContext {
    /// Create context from executor state
    pub fn from_executor(executor: &colony_exec::Executor, model: Option<String>) -> Self {
        let jobs = executor.jobs().list_jobs();
        let running = jobs.iter().filter(|j| j.is_running()).count();
        let stopped = jobs.iter().filter(|j| j.is_stopped()).count();

        Self {
            cwd: std::env::current_dir().unwrap_or_default(),
            env: std::env::vars().collect(),
            last_exit_code: executor.context().last_exit_status(),
            running_jobs: running,
            stopped_jobs: stopped,
            model,
        }
    }

    /// Get an environment variable
    pub fn get_env(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(|s| s.as_str())
    }

    /// Get home directory (from HOME env var)
    pub fn home_dir(&self) -> Option<&str> {
        self.get_env("HOME")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let ctx = ComponentContext {
            cwd: std::path::PathBuf::from("/home/user"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        };
        assert_eq!(ctx.cwd.to_str(), Some("/home/user"));
        assert_eq!(ctx.last_exit_code, 0);
    }

    #[test]
    fn test_context_with_model() {
        let ctx = ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: Some("gpt-4".to_string()),
        };
        assert_eq!(ctx.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_get_env() {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/home/test".to_string());
        env.insert("USER".to_string(), "test".to_string());

        let ctx = ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env,
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        };

        assert_eq!(ctx.get_env("HOME"), Some("/home/test"));
        assert_eq!(ctx.get_env("USER"), Some("test"));
        assert_eq!(ctx.get_env("NONEXISTENT"), None);
    }

    #[test]
    fn test_home_dir() {
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/Users/testuser".to_string());

        let ctx = ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env,
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        };

        assert_eq!(ctx.home_dir(), Some("/Users/testuser"));
    }
}
