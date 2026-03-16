//! Checklist parser for HEARTBEAT.md files.
//!
//! Parses optional YAML frontmatter (delimited by `---`) for configuration
//! and Markdown checkbox items (`- [ ]` / `- [x]`) as checklist entries.
//!
//! The frontmatter is parsed with simple line-by-line key-value extraction
//! (no full YAML dependency). Duration strings like `5m`, `1h`, `1d` are
//! supported for interval and timeout fields.

use super::types::{ChecklistItem, HeartbeatConfig, SecurityMode};

/// Parse a HEARTBEAT.md file into configuration and checklist items.
pub fn parse_heartbeat(content: &str) -> (HeartbeatConfig, Vec<ChecklistItem>) {
    let (frontmatter, body) = split_frontmatter(content);
    let config = parse_frontmatter(frontmatter);
    let items = parse_checklist(body);
    (config, items)
}

/// Check whether the body (after frontmatter) contains only headers and whitespace.
///
/// Returns `true` if there are no checklist items or substantive content,
/// indicating the scheduler should skip this tick.
pub fn body_is_empty(body: &str) -> bool {
    body.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with('#')
    })
}

/// Parse a duration string into seconds.
///
/// Supports: `5m`, `15m`, `30m`, `1h`, `4h`, `1d`, `60s`, `120s`.
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (digits, suffix) = s.split_at(s.len().saturating_sub(1));
    let value: u64 = digits.parse().ok()?;

    match suffix {
        "s" => Some(value),
        "m" => Some(value * 60),
        "h" => Some(value * 3600),
        "d" => Some(value * 86400),
        _ => None,
    }
}

/// Split content into optional frontmatter and body.
///
/// Frontmatter is delimited by `---` at the start and end.
/// If no frontmatter is found, the entire content is treated as body.
fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let trimmed = content.trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (None, content);
    };
    let rest = rest.trim_start_matches(['\r', '\n']);

    // Find the closing ---
    let Some(end_idx) = rest.find("\n---") else {
        // No closing delimiter: treat the whole thing as body
        return (None, content);
    };

    let frontmatter = &rest[..end_idx];
    // Skip past the closing --- and any trailing newline
    let after_close = &rest[end_idx + 4..]; // "\n---" is 4 bytes
    let body = after_close.trim_start_matches(['\r', '\n']);

    (Some(frontmatter), body)
}

/// Parse frontmatter key-value pairs into a `HeartbeatConfig`.
///
/// Unknown keys are silently ignored. Missing keys use defaults.
fn parse_frontmatter(yaml: Option<&str>) -> HeartbeatConfig {
    let mut config = HeartbeatConfig::default();

    let Some(yaml) = yaml else {
        return config;
    };

    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        match key {
            "interval" => {
                if let Some(secs) = parse_duration(value) {
                    config.interval_secs = secs;
                }
            }
            "mode" => {
                config.mode = SecurityMode::from_str_lossy(value);
            }
            "notify" => {
                config.notify = value == "true" || value == "yes";
            }
            "timeout" => {
                if let Some(secs) = parse_duration(value) {
                    config.timeout_secs = secs;
                }
            }
            "max_tool_calls" => {
                if let Ok(n) = value.parse::<u32>() {
                    config.max_tool_calls = n;
                }
            }
            "token_budget" => {
                if let Ok(n) = value.parse::<u32>() {
                    config.token_budget = n;
                }
            }
            _ => {} // Unknown keys silently ignored
        }
    }

    config
}

/// Parse checklist items from the body of a HEARTBEAT.md file.
///
/// Extracts lines matching `- [ ] text` (unchecked) and `- [x] text` (checked).
fn parse_checklist(body: &str) -> Vec<ChecklistItem> {
    let mut items = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();

        if let Some(text) = trimmed.strip_prefix("- [ ] ") {
            items.push(ChecklistItem {
                text: text.to_string(),
                checked: false,
            });
        } else if let Some(text) = trimmed.strip_prefix("- [x] ").or_else(|| trimmed.strip_prefix("- [X] ")) {
            items.push(ChecklistItem {
                text: text.to_string(),
                checked: true,
            });
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_with_frontmatter_and_items() {
        let content = "---\ninterval: 15m\nmode: active\ntimeout: 120s\n---\n\n# Heartbeat\n\n- [ ] Check tests\n- [x] Already done\n- [ ] Check disk\n";
        let (config, items) = parse_heartbeat(content);
        assert_eq!(config.interval_secs, 900);
        assert_eq!(config.mode, SecurityMode::Active);
        assert_eq!(config.timeout_secs, 120);
        assert_eq!(items.len(), 3);
        assert!(!items[0].checked);
        assert_eq!(items[0].text, "Check tests");
        assert!(items[1].checked);
        assert_eq!(items[1].text, "Already done");
        assert!(!items[2].checked);
        assert_eq!(items[2].text, "Check disk");
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "# Heartbeat\n\n- [ ] Check tests\n";
        let (config, items) = parse_heartbeat(content);
        assert_eq!(config.interval_secs, 1800); // default 30m
        assert_eq!(config.mode, SecurityMode::Notify);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_parse_empty_body() {
        let content = "---\ninterval: 5m\n---\n\n# Heartbeat\n\n";
        let (config, items) = parse_heartbeat(content);
        assert_eq!(config.interval_secs, 300);
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_duration_formats() {
        assert_eq!(parse_duration("5m"), Some(300));
        assert_eq!(parse_duration("15m"), Some(900));
        assert_eq!(parse_duration("30m"), Some(1800));
        assert_eq!(parse_duration("1h"), Some(3600));
        assert_eq!(parse_duration("4h"), Some(14400));
        assert_eq!(parse_duration("1d"), Some(86400));
        assert_eq!(parse_duration("60s"), Some(60));
        assert_eq!(parse_duration("120s"), Some(120));
        assert_eq!(parse_duration("garbage"), None);
    }

    #[test]
    fn test_active_items_only() {
        let content = "- [ ] Active item\n- [x] Done item\n";
        let (_, items) = parse_heartbeat(content);
        let active: Vec<_> = items.iter().filter(|i| !i.checked).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].text, "Active item");
    }

    #[test]
    fn test_body_is_empty_check() {
        assert!(body_is_empty("# Heartbeat\n\n## Section\n\n"));
        assert!(body_is_empty("   \n\n  \n"));
        assert!(body_is_empty(""));
        assert!(!body_is_empty("- [ ] Check something\n"));
    }

    #[test]
    fn test_parse_all_config_fields() {
        let content = "---\ninterval: 1h\nmode: full\nnotify: false\ntimeout: 90s\nmax_tool_calls: 10\ntoken_budget: 5000\n---\n\n- [ ] Item\n";
        let (config, items) = parse_heartbeat(content);
        assert_eq!(config.interval_secs, 3600);
        assert_eq!(config.mode, SecurityMode::Full);
        assert!(!config.notify);
        assert_eq!(config.timeout_secs, 90);
        assert_eq!(config.max_tool_calls, 10);
        assert_eq!(config.token_budget, 5000);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_parse_uppercase_x_checked() {
        let content = "- [X] Uppercase checked\n- [x] Lowercase checked\n";
        let (_, items) = parse_heartbeat(content);
        assert_eq!(items.len(), 2);
        assert!(items[0].checked);
        assert!(items[1].checked);
    }

    #[test]
    fn test_parse_items_with_commands() {
        let content = "- [ ] Check tests (`cargo test 2>&1 | tail -5`)\n- [ ] Verify server (`curl -s localhost:8080/health`)\n";
        let (_, items) = parse_heartbeat(content);
        assert_eq!(items.len(), 2);
        assert!(items[0].text.contains("cargo test"));
        assert!(items[1].text.contains("curl"));
    }

    #[test]
    fn test_frontmatter_with_unknown_keys() {
        let content = "---\ninterval: 10m\nunknown_key: whatever\nmode: notify\n---\n\n- [ ] Item\n";
        let (config, _) = parse_heartbeat(content);
        assert_eq!(config.interval_secs, 600);
        assert_eq!(config.mode, SecurityMode::Notify);
    }

    #[test]
    fn test_split_frontmatter_no_close() {
        let content = "---\ninterval: 5m\nno closing delimiter\n";
        let (fm, _body) = split_frontmatter(content);
        assert!(fm.is_none());
    }

    #[test]
    fn test_split_frontmatter_proper() {
        let content = "---\ninterval: 5m\n---\n\nbody here\n";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_some());
        assert!(fm.unwrap().contains("interval: 5m"));
        assert!(body.contains("body here"));
    }
}
