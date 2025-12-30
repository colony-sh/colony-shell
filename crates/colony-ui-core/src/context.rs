use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ComponentContext {
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub last_exit_code: i32,
    pub running_jobs: usize,
    pub stopped_jobs: usize,
    pub model: Option<String>,
    pub last_command_duration: Option<Duration>,
    pub hostname: Option<String>,
}

impl ComponentContext {
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
            last_command_duration: None,
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
        }
    }

    pub fn get_env(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(|s| s.as_str())
    }

    pub fn home_dir(&self) -> Option<&str> {
        self.get_env("HOME")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ComponentContext {
        ComponentContext {
            cwd: PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
            last_command_duration: None,
            hostname: None,
        }
    }

    #[test]
    fn test_context_creation() {
        let mut ctx = test_ctx();
        ctx.cwd = PathBuf::from("/home/user");
        assert_eq!(ctx.cwd.to_str(), Some("/home/user"));
        assert_eq!(ctx.last_exit_code, 0);
    }

    #[test]
    fn test_context_with_model() {
        let mut ctx = test_ctx();
        ctx.model = Some("gpt-4".to_string());
        assert_eq!(ctx.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_get_env() {
        let mut ctx = test_ctx();
        ctx.env.insert("HOME".to_string(), "/home/test".to_string());
        ctx.env.insert("USER".to_string(), "test".to_string());

        assert_eq!(ctx.get_env("HOME"), Some("/home/test"));
        assert_eq!(ctx.get_env("USER"), Some("test"));
        assert_eq!(ctx.get_env("NONEXISTENT"), None);
    }

    #[test]
    fn test_home_dir() {
        let mut ctx = test_ctx();
        ctx.env.insert("HOME".to_string(), "/Users/testuser".to_string());
        assert_eq!(ctx.home_dir(), Some("/Users/testuser"));
    }
}
