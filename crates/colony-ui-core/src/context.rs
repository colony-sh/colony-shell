use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ComponentContext {
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub last_exit_code: i32,
    pub running_jobs: usize,
    pub stopped_jobs: usize,
    pub model: Option<String>,
}
