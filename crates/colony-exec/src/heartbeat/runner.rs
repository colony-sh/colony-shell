//! Per-tick execution: finding extraction and status derivation.
//!
//! Parses agent output for structured `PASS:`, `WARN:`, and `ALERT:` lines
//! and derives the overall heartbeat status from the collected findings.

use super::types::{Finding, HeartbeatStatus, Severity};

/// Extract structured findings from agent output text.
///
/// Lines starting with `PASS:`, `WARN:`, or `ALERT:` (case-insensitive)
/// are parsed into `Finding` structs. All other content is ignored by
/// the extractor but preserved in `raw_output` on the `HeartbeatRun`.
pub fn extract_findings(output: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if let Some(msg) = strip_prefix_case_insensitive(trimmed, "PASS:") {
            findings.push(Finding {
                severity: Severity::Pass,
                message: msg.trim().to_string(),
            });
        } else if let Some(msg) = strip_prefix_case_insensitive(trimmed, "WARN:") {
            findings.push(Finding {
                severity: Severity::Warn,
                message: msg.trim().to_string(),
            });
        } else if let Some(msg) = strip_prefix_case_insensitive(trimmed, "ALERT:") {
            findings.push(Finding {
                severity: Severity::Alert,
                message: msg.trim().to_string(),
            });
        }
    }

    findings
}

/// Derive the overall heartbeat status from a list of findings.
///
/// - If no findings exist, status is `Unstructured`.
/// - If any finding is `Alert`, status is `Alert`.
/// - If any finding is `Warn` (and no `Alert`), status is `Warn`.
/// - If all findings are `Pass`, status is `Pass`.
pub fn derive_status(findings: &[Finding]) -> HeartbeatStatus {
    if findings.is_empty() {
        return HeartbeatStatus::Unstructured;
    }

    let has_alert = findings.iter().any(|f| f.severity == Severity::Alert);
    let has_warn = findings.iter().any(|f| f.severity == Severity::Warn);

    if has_alert {
        HeartbeatStatus::Alert
    } else if has_warn {
        HeartbeatStatus::Warn
    } else {
        HeartbeatStatus::Pass
    }
}

/// Build the system prompt for the heartbeat agent.
pub fn build_system_prompt() -> String {
    "You are a heartbeat monitor for a software project. \
     Execute each checklist item, report findings concisely. \
     Do not infer tasks from prior runs. \
     \n\nFormat each finding on its own line as:\n\
     PASS: <description>\n\
     WARN: <description>\n\
     ALERT: <description>\n\n\
     Use PASS for checks that succeed, WARN for non-critical issues, \
     and ALERT for critical problems that need immediate attention."
        .to_string()
}

/// Build the user message containing the checklist and context.
pub fn build_checklist_prompt(
    checklist_content: &str,
    context_content: Option<&str>,
    cwd: &str,
    git_branch: Option<&str>,
) -> String {
    let mut prompt = String::with_capacity(checklist_content.len() + 256);

    prompt.push_str("## Current Directory\n");
    prompt.push_str(cwd);
    prompt.push('\n');

    if let Some(branch) = git_branch {
        prompt.push_str("\n## Git Branch\n");
        prompt.push_str(branch);
        prompt.push('\n');
    }

    prompt.push_str("\n## Timestamp\n");
    prompt.push_str(&chrono::Utc::now().to_rfc3339());
    prompt.push('\n');

    if let Some(context) = context_content {
        if !context.is_empty() {
            prompt.push_str("\n## Project Context\n");
            prompt.push_str(context);
            prompt.push('\n');
        }
    }

    prompt.push_str("\n## Checklist\n");
    prompt.push_str(checklist_content);

    prompt
}

/// Case-insensitive prefix strip.
fn strip_prefix_case_insensitive<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let lower = s.get(..prefix.len())?;
    if lower.eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_findings_basic() {
        let output = "\
PASS: Tests are passing (147/147)
WARN: 3 new TODO items found in src/main.rs
ALERT: Dev server is not responding on localhost:8080
PASS: Disk usage normal (2.1G target/)
";
        let findings = extract_findings(output);
        assert_eq!(findings.len(), 4);
        assert_eq!(findings[0].severity, Severity::Pass);
        assert_eq!(findings[0].message, "Tests are passing (147/147)");
        assert_eq!(findings[1].severity, Severity::Warn);
        assert_eq!(findings[2].severity, Severity::Alert);
        assert_eq!(findings[3].severity, Severity::Pass);
    }

    #[test]
    fn test_extract_findings_mixed_content() {
        let output = "\
Here are the results of my checks:

PASS: All tests passing
Some additional commentary here.
WARN: Found 2 unused imports

I also noticed some other things but they're not critical.
";
        let findings = extract_findings(output);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].severity, Severity::Pass);
        assert_eq!(findings[1].severity, Severity::Warn);
    }

    #[test]
    fn test_extract_findings_empty() {
        let output = "No structured findings here, just prose.";
        let findings = extract_findings(output);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_extract_findings_case_insensitive() {
        let output = "pass: lowercase pass\nWARN: uppercase warn\nAlert: mixed case alert\n";
        let findings = extract_findings(output);
        assert_eq!(findings.len(), 3);
        assert_eq!(findings[0].severity, Severity::Pass);
        assert_eq!(findings[1].severity, Severity::Warn);
        assert_eq!(findings[2].severity, Severity::Alert);
    }

    #[test]
    fn test_derive_status_pass() {
        let findings = vec![
            Finding {
                severity: Severity::Pass,
                message: "ok".to_string(),
            },
            Finding {
                severity: Severity::Pass,
                message: "ok".to_string(),
            },
        ];
        assert_eq!(derive_status(&findings), HeartbeatStatus::Pass);
    }

    #[test]
    fn test_derive_status_warn() {
        let findings = vec![
            Finding {
                severity: Severity::Pass,
                message: "ok".to_string(),
            },
            Finding {
                severity: Severity::Warn,
                message: "hmm".to_string(),
            },
        ];
        assert_eq!(derive_status(&findings), HeartbeatStatus::Warn);
    }

    #[test]
    fn test_derive_status_alert() {
        let findings = vec![
            Finding {
                severity: Severity::Pass,
                message: "ok".to_string(),
            },
            Finding {
                severity: Severity::Warn,
                message: "hmm".to_string(),
            },
            Finding {
                severity: Severity::Alert,
                message: "bad".to_string(),
            },
        ];
        assert_eq!(derive_status(&findings), HeartbeatStatus::Alert);
    }

    #[test]
    fn test_derive_status_unstructured() {
        let findings: Vec<Finding> = vec![];
        assert_eq!(derive_status(&findings), HeartbeatStatus::Unstructured);
    }

    #[test]
    fn test_build_system_prompt() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("heartbeat monitor"));
        assert!(prompt.contains("PASS:"));
        assert!(prompt.contains("WARN:"));
        assert!(prompt.contains("ALERT:"));
    }

    #[test]
    fn test_build_checklist_prompt() {
        let checklist = "- [ ] Check tests\n- [ ] Check disk\n";
        let prompt = build_checklist_prompt(
            checklist,
            Some("Rust project"),
            "/home/user/project",
            Some("main"),
        );
        assert!(prompt.contains("Check tests"));
        assert!(prompt.contains("/home/user/project"));
        assert!(prompt.contains("main"));
        assert!(prompt.contains("Rust project"));
    }

    #[test]
    fn test_build_checklist_prompt_minimal() {
        let checklist = "- [ ] Check tests\n";
        let prompt = build_checklist_prompt(checklist, None, "/tmp", None);
        assert!(prompt.contains("Check tests"));
        assert!(prompt.contains("/tmp"));
        assert!(!prompt.contains("Git Branch"));
    }
}
