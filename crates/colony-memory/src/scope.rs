use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Scope {
    #[default]
    Global,
    Project(String),
    Session(String),
    Task(String),
}

impl Scope {
    pub fn as_str(&self) -> &str {
        match self {
            Scope::Global => "global",
            Scope::Project(_) => "project",
            Scope::Session(_) => "session",
            Scope::Task(_) => "task",
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Scope::Global => None,
            Scope::Project(id) | Scope::Session(id) | Scope::Task(id) => Some(id),
        }
    }

    pub fn priority(&self) -> u8 {
        match self {
            Scope::Global => 0,
            Scope::Project(_) => 1,
            Scope::Session(_) => 2,
            Scope::Task(_) => 3,
        }
    }

    pub fn from_parts(scope_type: &str, scope_id: Option<&str>) -> Option<Self> {
        match scope_type {
            "global" => Some(Scope::Global),
            "project" => scope_id.map(|id| Scope::Project(id.to_string())),
            "session" => scope_id.map(|id| Scope::Session(id.to_string())),
            "task" => scope_id.map(|id| Scope::Task(id.to_string())),
            _ => None,
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Global => write!(f, "global"),
            Scope::Project(id) => write!(f, "project:{}", id),
            Scope::Session(id) => write!(f, "session:{}", id),
            Scope::Task(id) => write!(f, "task:{}", id),
        }
    }
}
