//! Tool approval enforcement
//!
//! Checks tool calls against recipe-defined approval policies.
//! Deny patterns take priority over approve patterns.

use crate::recipe::ToolApprovalConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    AutoApprove,
    RequireApproval,
    Deny(String),
}

impl std::fmt::Display for ApprovalDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AutoApprove => write!(f, "auto-approved"),
            Self::RequireApproval => write!(f, "requires approval"),
            Self::Deny(reason) => write!(f, "denied: {reason}"),
        }
    }
}

pub struct ApprovalEnforcer {
    config: ToolApprovalConfig,
}

impl ApprovalEnforcer {
    pub fn new(config: ToolApprovalConfig) -> Self {
        Self { config }
    }

    pub fn check(&self, tool_name: &str) -> ApprovalDecision {
        if self.matches_any(tool_name, &self.config.deny) {
            return ApprovalDecision::Deny(format!(
                "tool '{tool_name}' is denied by recipe policy"
            ));
        }

        if self.matches_any(tool_name, &self.config.auto_approve) {
            return ApprovalDecision::AutoApprove;
        }

        if self.matches_any(tool_name, &self.config.require_approval) {
            return ApprovalDecision::RequireApproval;
        }

        ApprovalDecision::RequireApproval
    }

    pub fn is_denied(&self, tool_name: &str) -> bool {
        matches!(self.check(tool_name), ApprovalDecision::Deny(_))
    }

    pub fn is_auto_approved(&self, tool_name: &str) -> bool {
        self.check(tool_name) == ApprovalDecision::AutoApprove
    }

    fn matches_any(&self, tool_name: &str, patterns: &[String]) -> bool {
        patterns.iter().any(|p| pattern_matches(p, tool_name))
    }
}

fn pattern_matches(pattern: &str, tool_name: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.contains('*') {
        return glob_match(pattern, tool_name);
    }

    pattern == tool_name
}

fn glob_match(pattern: &str, value: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 2 {
        let prefix = parts[0];
        let suffix = parts[1];

        if !prefix.is_empty() && !value.starts_with(prefix) {
            return false;
        }
        if !suffix.is_empty() && !value.ends_with(suffix) {
            return false;
        }
        if !prefix.is_empty() && !suffix.is_empty() {
            return value.len() >= prefix.len() + suffix.len();
        }
        return true;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = value[pos..].find(part) else {
            return false;
        };
        if i == 0 && !pattern.starts_with('*') && found != 0 {
            return false;
        }
        pos += found + part.len();
    }

    if !pattern.ends_with('*') {
        return pos == value.len();
    }

    true
}

impl Default for ApprovalEnforcer {
    fn default() -> Self {
        Self::new(ToolApprovalConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(
        auto_approve: Vec<&str>,
        require_approval: Vec<&str>,
        deny: Vec<&str>,
    ) -> ToolApprovalConfig {
        ToolApprovalConfig {
            auto_approve: auto_approve.into_iter().map(String::from).collect(),
            require_approval: require_approval.into_iter().map(String::from).collect(),
            deny: deny.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn test_deny_takes_priority() {
        let config = make_config(vec!["*"], vec![], vec!["dangerous_tool"]);
        let enforcer = ApprovalEnforcer::new(config);

        assert!(enforcer.is_denied("dangerous_tool"));
        assert!(enforcer.is_auto_approved("safe_tool"));
    }

    #[test]
    fn test_auto_approve_exact() {
        let config = make_config(vec!["read_file", "list_dir"], vec![], vec![]);
        let enforcer = ApprovalEnforcer::new(config);

        assert_eq!(enforcer.check("read_file"), ApprovalDecision::AutoApprove);
        assert_eq!(enforcer.check("list_dir"), ApprovalDecision::AutoApprove);
        assert_eq!(
            enforcer.check("write_file"),
            ApprovalDecision::RequireApproval
        );
    }

    #[test]
    fn test_auto_approve_wildcard() {
        let config = make_config(vec!["read_*"], vec![], vec![]);
        let enforcer = ApprovalEnforcer::new(config);

        assert_eq!(enforcer.check("read_file"), ApprovalDecision::AutoApprove);
        assert_eq!(enforcer.check("read_dir"), ApprovalDecision::AutoApprove);
        assert_eq!(
            enforcer.check("write_file"),
            ApprovalDecision::RequireApproval
        );
    }

    #[test]
    fn test_deny_wildcard() {
        let config = make_config(vec![], vec![], vec!["rm_*"]);
        let enforcer = ApprovalEnforcer::new(config);

        assert!(enforcer.is_denied("rm_rf"));
        assert!(enforcer.is_denied("rm_file"));
        assert!(!enforcer.is_denied("read_file"));
    }

    #[test]
    fn test_deny_star_overrides_approve_star() {
        let config = make_config(vec!["*"], vec![], vec!["*"]);
        let enforcer = ApprovalEnforcer::new(config);

        assert!(enforcer.is_denied("anything"));
    }

    #[test]
    fn test_require_approval_explicit() {
        let config = make_config(vec![], vec!["write_file"], vec![]);
        let enforcer = ApprovalEnforcer::new(config);

        assert_eq!(
            enforcer.check("write_file"),
            ApprovalDecision::RequireApproval
        );
    }

    #[test]
    fn test_default_is_require_approval() {
        let config = make_config(vec![], vec![], vec![]);
        let enforcer = ApprovalEnforcer::new(config);

        assert_eq!(
            enforcer.check("any_tool"),
            ApprovalDecision::RequireApproval
        );
    }

    #[test]
    fn test_glob_match_prefix() {
        assert!(glob_match("read_*", "read_file"));
        assert!(glob_match("read_*", "read_"));
        assert!(!glob_match("read_*", "write_file"));
    }

    #[test]
    fn test_glob_match_suffix() {
        assert!(glob_match("*_file", "read_file"));
        assert!(glob_match("*_file", "write_file"));
        assert!(!glob_match("*_file", "read_dir"));
    }

    #[test]
    fn test_glob_match_middle() {
        assert!(glob_match("mcp_*_read", "mcp_fs_read"));
        assert!(!glob_match("mcp_*_read", "mcp_fs_write"));
    }

    #[test]
    fn test_glob_match_star_only() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
    }

    #[test]
    fn test_pattern_matches_exact() {
        assert!(pattern_matches("read_file", "read_file"));
        assert!(!pattern_matches("read_file", "write_file"));
    }

    #[test]
    fn test_approval_decision_display() {
        assert_eq!(ApprovalDecision::AutoApprove.to_string(), "auto-approved");
        assert_eq!(
            ApprovalDecision::RequireApproval.to_string(),
            "requires approval"
        );
        assert_eq!(
            ApprovalDecision::Deny("blocked".to_string()).to_string(),
            "denied: blocked"
        );
    }

    #[test]
    fn test_complex_policy() {
        let config = make_config(
            vec!["read_*", "list_*"],
            vec!["write_*"],
            vec!["rm_rf", "exec_*"],
        );
        let enforcer = ApprovalEnforcer::new(config);

        assert_eq!(enforcer.check("read_file"), ApprovalDecision::AutoApprove);
        assert_eq!(enforcer.check("list_dir"), ApprovalDecision::AutoApprove);
        assert_eq!(
            enforcer.check("write_file"),
            ApprovalDecision::RequireApproval
        );
        assert!(enforcer.is_denied("rm_rf"));
        assert!(enforcer.is_denied("exec_command"));
        assert_eq!(enforcer.check("unknown"), ApprovalDecision::RequireApproval);
    }

    #[test]
    fn test_default_enforcer() {
        let enforcer = ApprovalEnforcer::default();
        assert_eq!(
            enforcer.check("anything"),
            ApprovalDecision::RequireApproval
        );
    }
}
