//! Static rule-based risk scoring for agent-originated commands.

use regex::Regex;

use crate::record::RiskLevel;

/// A single risk scoring rule: pattern + risk level.
struct RiskRule {
    pattern: Regex,
    level: RiskLevel,
}

/// Scores commands by risk level using ordered regex rules.
///
/// Rules are evaluated in order -- the first match determines the risk
/// level. If no rules match, the command defaults to `Medium`.
pub struct RiskScorer {
    rules: Vec<RiskRule>,
}

impl Default for RiskScorer {
    fn default() -> Self {
        let rule_defs: Vec<(&str, RiskLevel)> = vec![
            // Critical
            (
                r"(?:^|\s)rm\s+.*-r.*-f|(?:^|\s)rm\s+.*-rf",
                RiskLevel::Critical,
            ),
            (r"(?:^|\s)sudo\s", RiskLevel::Critical),
            (r"chmod\s+777", RiskLevel::Critical),
            (r"(?i)DROP\s+TABLE", RiskLevel::Critical),
            (r"(?:>|>>)\s*/etc/", RiskLevel::Critical),
            // High
            (r"git\s+push\s+--force", RiskLevel::High),
            (r"git\s+push\s+-f\b", RiskLevel::High),
            (r"docker\s+rm\b", RiskLevel::High),
            (
                r"(?:npm|yarn|pnpm|cargo)\s+(?:install|add|remove|uninstall)",
                RiskLevel::High,
            ),
            (r"pip\s+(?:install|uninstall)", RiskLevel::High),
            // Medium
            (r"git\s+commit", RiskLevel::Medium),
            (r"(?:>|>>)\s", RiskLevel::Medium),
            (r"curl\s|wget\s|fetch\s", RiskLevel::Medium),
            (r"cp\s|mv\s", RiskLevel::Medium),
            // Low
            (r"(?:cat|head|tail|less|more)\s", RiskLevel::Low),
            (r"git\s+(?:status|diff|log|show|branch)", RiskLevel::Low),
            (r"(?:find|grep|rg|ag)\s", RiskLevel::Low),
            // Safe (explicit matches)
            (
                r"^(?:echo|printf|pwd|ls|tree|history|type|which|alias)\b",
                RiskLevel::Safe,
            ),
        ];

        let rules = rule_defs
            .into_iter()
            .map(|(pattern, level)| RiskRule {
                pattern: Regex::new(pattern).expect("invalid risk pattern"),
                level,
            })
            .collect();

        Self { rules }
    }
}

impl RiskScorer {
    /// Score a command string by its risk level.
    ///
    /// Returns the level of the first matching rule, or `Medium` if no
    /// rules match.
    pub fn score(&self, command: &str) -> RiskLevel {
        for rule in &self.rules {
            if rule.pattern.is_match(command) {
                return rule.level.clone();
            }
        }
        // Default: medium for unknown commands
        RiskLevel::Medium
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_critical_rm_rf() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("rm -rf /"), RiskLevel::Critical);
        assert_eq!(scorer.score("rm -rf /tmp/foo"), RiskLevel::Critical);
    }

    #[test]
    fn test_critical_sudo() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("sudo apt install foo"), RiskLevel::Critical);
    }

    #[test]
    fn test_critical_chmod_777() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("chmod 777 /etc/passwd"), RiskLevel::Critical);
    }

    #[test]
    fn test_critical_drop_table() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("DROP TABLE users"), RiskLevel::Critical);
    }

    #[test]
    fn test_high_git_push_force() {
        let scorer = RiskScorer::default();
        assert_eq!(
            scorer.score("git push --force origin main"),
            RiskLevel::High
        );
    }

    #[test]
    fn test_high_docker_rm() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("docker rm container1"), RiskLevel::High);
    }

    #[test]
    fn test_high_npm_install() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("npm install express"), RiskLevel::High);
    }

    #[test]
    fn test_medium_git_commit() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("git commit -m 'message'"), RiskLevel::Medium);
    }

    #[test]
    fn test_medium_curl() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("curl https://example.com"), RiskLevel::Medium);
    }

    #[test]
    fn test_low_cat() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("cat README.md"), RiskLevel::Low);
    }

    #[test]
    fn test_low_git_status() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("git status"), RiskLevel::Low);
    }

    #[test]
    fn test_low_git_diff() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("git diff"), RiskLevel::Low);
    }

    #[test]
    fn test_low_git_log() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("git log"), RiskLevel::Low);
    }

    #[test]
    fn test_safe_echo() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("echo hello"), RiskLevel::Safe);
    }

    #[test]
    fn test_safe_ls() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("ls -la"), RiskLevel::Safe);
    }

    #[test]
    fn test_safe_pwd() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("pwd"), RiskLevel::Safe);
    }

    #[test]
    fn test_unknown_command_defaults_to_medium() {
        let scorer = RiskScorer::default();
        assert_eq!(scorer.score("some-unknown-tool --flag"), RiskLevel::Medium);
    }
}
