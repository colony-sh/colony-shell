//! Event worker for evaluating tuple space events against rules from colony-nerve.
//!
//! Colony-nerve sends `EventRules` messages over the widget server Unix socket.
//! This module stores those rules and evaluates incoming tuple space events,
//! emitting `event_result` OSC 2337 messages back to nerve.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::Deserialize;

/// Specification for a single event rule received from colony-nerve.
#[derive(Debug, Clone, Deserialize)]
pub struct EventRuleSpec {
    /// Event name to match (e.g. "file_changed", "agent_status").
    pub event: String,
    /// Conditions that must all pass for the rule to fire.
    pub conditions: Vec<ConditionSpec>,
    /// Action to execute when the rule fires.
    pub action: ActionSpec,
}

/// A single condition to evaluate against event fields.
#[derive(Debug, Clone, Deserialize)]
pub struct ConditionSpec {
    /// JSON field path to check.
    pub field: String,
    /// Comparison operator: "contains", "is", ">", "<", ">=", "<=", "matches".
    pub op: String,
    /// Value to compare against.
    pub value: String,
}

/// Action to execute when a rule's conditions are met.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ActionSpec {
    /// Emit a notification message.
    #[serde(rename = "notify")]
    Notify { message: String },
    /// Log the event fields.
    #[serde(rename = "log")]
    Log,
    /// Run a shell command.
    #[serde(rename = "run")]
    Run { command: String },
    /// Send a webhook (placeholder, no HTTP client).
    #[serde(rename = "webhook")]
    Webhook { url: String },
    /// Request a plugin refresh.
    #[serde(rename = "refresh")]
    Refresh { plugin: String },
    /// Trigger a plugin action with parameters.
    #[serde(rename = "plugin_action")]
    PluginAction {
        plugin: String,
        action: String,
        params: HashMap<String, String>,
    },
}

/// Manages event rules and evaluates them against tuple space events.
pub struct EventWorker {
    rules: Arc<Mutex<Vec<EventRuleSpec>>>,
}

impl EventWorker {
    /// Create a new event worker with no rules.
    pub fn new() -> Self {
        Self {
            rules: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Deserialize and store rules received from colony-nerve.
    ///
    /// Replaces any previously stored rules. Invalid rules are logged and skipped.
    pub fn update_rules(&self, rules: Vec<serde_json::Value>) {
        let parsed: Vec<EventRuleSpec> = rules
            .into_iter()
            .enumerate()
            .filter_map(|(i, val)| {
                serde_json::from_value::<EventRuleSpec>(val)
                    .map_err(|e| {
                        tracing::warn!("failed to parse event rule {i}: {e}");
                    })
                    .ok()
            })
            .collect();

        let count = parsed.len();
        *self.rules.lock() = parsed;
        tracing::info!("event worker updated with {count} rules");
    }

    /// Evaluate all rules against an incoming event.
    ///
    /// For each matching rule, the action is executed and an `event_result`
    /// OSC 2337 message is emitted.
    pub fn evaluate(&self, event_name: &str, fields: &serde_json::Value) {
        let rules = self.rules.lock().clone();

        for (i, rule) in rules.iter().enumerate() {
            if rule.event != event_name {
                continue;
            }

            if !check_conditions(&rule.conditions, fields) {
                continue;
            }

            let status = execute_action(&rule.action, fields);
            emit_event_result(i, event_name, &status);
        }
    }
}

/// Check that all conditions pass against the given fields.
fn check_conditions(conditions: &[ConditionSpec], fields: &serde_json::Value) -> bool {
    conditions
        .iter()
        .all(|cond| check_one_condition(cond, fields))
}

/// Evaluate a single condition against the event fields.
fn check_one_condition(cond: &ConditionSpec, fields: &serde_json::Value) -> bool {
    let Some(field_val) = fields.get(&cond.field) else {
        return false;
    };

    let field_str = match field_val {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    match cond.op.as_str() {
        "contains" => field_str.contains(&cond.value),
        "is" => field_str == cond.value,
        ">" | "<" | ">=" | "<=" => check_numeric_condition(&field_str, &cond.op, &cond.value),
        "matches" => regex::Regex::new(&cond.value)
            .map(|re| re.is_match(&field_str))
            .unwrap_or(false),
        unknown => {
            tracing::warn!("unknown condition operator: {unknown}");
            false
        }
    }
}

/// Compare two values as f64 using the given numeric operator.
fn check_numeric_condition(field_str: &str, op: &str, value: &str) -> bool {
    let Ok(lhs) = field_str.parse::<f64>() else {
        return false;
    };
    let Ok(rhs) = value.parse::<f64>() else {
        return false;
    };

    match op {
        ">" => lhs > rhs,
        "<" => lhs < rhs,
        ">=" => lhs >= rhs,
        "<=" => lhs <= rhs,
        _ => false,
    }
}

/// Execute an action and return a status string.
fn execute_action(action: &ActionSpec, fields: &serde_json::Value) -> String {
    match action {
        ActionSpec::Notify { message } => {
            let msg = interpolate(message, fields);
            tracing::info!("event notify: {msg}");
            emit_osc_action("notify", &serde_json::json!({ "message": msg }));
            "notified".to_string()
        }
        ActionSpec::Log => {
            tracing::info!("event log: {fields}");
            "logged".to_string()
        }
        ActionSpec::Run { command } => {
            let cmd = interpolate(command, fields);
            tracing::info!("event run: {cmd}");
            let result = std::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output();
            match result {
                Ok(output) => {
                    let code = output.status.code().unwrap_or(-1);
                    format!("exit:{code}")
                }
                Err(e) => format!("error:{e}"),
            }
        }
        ActionSpec::Webhook { url } => {
            tracing::info!("event webhook (skipped): {url}");
            "ok (skipped - no http client)".to_string()
        }
        ActionSpec::Refresh { plugin } => {
            tracing::info!("event refresh: {plugin}");
            emit_osc_action("refresh", &serde_json::json!({ "plugin": plugin }));
            "refreshed".to_string()
        }
        ActionSpec::PluginAction {
            plugin,
            action,
            params,
        } => {
            tracing::info!("event plugin_action: {plugin}.{action}");
            emit_osc_action(
                "plugin_action",
                &serde_json::json!({
                    "plugin": plugin,
                    "action": action,
                    "params": params,
                }),
            );
            "dispatched".to_string()
        }
    }
}

/// Emit an `event_result` OSC 2337 message to the terminal.
fn emit_event_result(rule_index: usize, event: &str, status: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let result = serde_json::json!({
        "type": "event_result",
        "rule_index": rule_index,
        "event": event,
        "status": status,
        "timestamp": timestamp,
    });

    let Ok(payload) = serde_json::to_string(&result) else {
        return;
    };
    print!("\x1b]2337;{payload}\x07");
}

/// Emit a raw OSC 2337 action message (for refresh, plugin_action, notify).
fn emit_osc_action(action_type: &str, data: &serde_json::Value) {
    let msg = serde_json::json!({
        "type": "event_action",
        "action": action_type,
        "data": data,
    });

    let Ok(payload) = serde_json::to_string(&msg) else {
        return;
    };
    print!("\x1b]2337;{payload}\x07");
}

/// Interpolate `${key}` placeholders in a template using event field values.
fn interpolate(template: &str, fields: &serde_json::Value) -> String {
    let Some(obj) = fields.as_object() else {
        return template.to_string();
    };

    let mut result = template.to_string();
    for (key, value) in obj {
        let placeholder = format!("${{{key}}}");
        let replacement = match value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        result = result.replace(&placeholder, &replacement);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_basic() {
        let fields = serde_json::json!({
            "name": "test.rs",
            "count": 42
        });
        let result = interpolate("file ${name} changed ${count} times", &fields);
        assert_eq!(result, "file test.rs changed 42 times");
    }

    #[test]
    fn test_interpolate_no_object() {
        let fields = serde_json::json!("just a string");
        let result = interpolate("hello ${name}", &fields);
        assert_eq!(result, "hello ${name}");
    }

    #[test]
    fn test_check_conditions_contains() {
        let cond = ConditionSpec {
            field: "path".to_string(),
            op: "contains".to_string(),
            value: ".rs".to_string(),
        };
        let fields = serde_json::json!({ "path": "src/main.rs" });
        assert!(check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_is() {
        let cond = ConditionSpec {
            field: "status".to_string(),
            op: "is".to_string(),
            value: "ready".to_string(),
        };
        let fields = serde_json::json!({ "status": "ready" });
        assert!(check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_is_mismatch() {
        let cond = ConditionSpec {
            field: "status".to_string(),
            op: "is".to_string(),
            value: "ready".to_string(),
        };
        let fields = serde_json::json!({ "status": "busy" });
        assert!(!check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_numeric_gt() {
        let cond = ConditionSpec {
            field: "count".to_string(),
            op: ">".to_string(),
            value: "5".to_string(),
        };
        let fields = serde_json::json!({ "count": 10 });
        assert!(check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_numeric_lt() {
        let cond = ConditionSpec {
            field: "count".to_string(),
            op: "<".to_string(),
            value: "5".to_string(),
        };
        let fields = serde_json::json!({ "count": 3 });
        assert!(check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_numeric_gte() {
        let cond = ConditionSpec {
            field: "count".to_string(),
            op: ">=".to_string(),
            value: "5".to_string(),
        };
        assert!(check_conditions(
            &[cond.clone()],
            &serde_json::json!({ "count": 5 })
        ));
        assert!(check_conditions(
            &[cond],
            &serde_json::json!({ "count": 6 })
        ));
    }

    #[test]
    fn test_check_conditions_numeric_lte() {
        let cond = ConditionSpec {
            field: "count".to_string(),
            op: "<=".to_string(),
            value: "5".to_string(),
        };
        assert!(check_conditions(
            &[cond.clone()],
            &serde_json::json!({ "count": 5 })
        ));
        assert!(check_conditions(
            &[cond],
            &serde_json::json!({ "count": 4 })
        ));
    }

    #[test]
    fn test_check_conditions_matches_regex() {
        let cond = ConditionSpec {
            field: "path".to_string(),
            op: "matches".to_string(),
            value: r"^src/.*\.rs$".to_string(),
        };
        let fields = serde_json::json!({ "path": "src/main.rs" });
        assert!(check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_matches_regex_no_match() {
        let cond = ConditionSpec {
            field: "path".to_string(),
            op: "matches".to_string(),
            value: r"^tests/".to_string(),
        };
        let fields = serde_json::json!({ "path": "src/main.rs" });
        assert!(!check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_missing_field() {
        let cond = ConditionSpec {
            field: "missing".to_string(),
            op: "is".to_string(),
            value: "anything".to_string(),
        };
        let fields = serde_json::json!({ "present": "value" });
        assert!(!check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_multiple_all_must_pass() {
        let conds = vec![
            ConditionSpec {
                field: "status".to_string(),
                op: "is".to_string(),
                value: "ready".to_string(),
            },
            ConditionSpec {
                field: "count".to_string(),
                op: ">".to_string(),
                value: "0".to_string(),
            },
        ];
        let fields = serde_json::json!({ "status": "ready", "count": 5 });
        assert!(check_conditions(&conds, &fields));

        let fields_fail = serde_json::json!({ "status": "busy", "count": 5 });
        assert!(!check_conditions(&conds, &fields_fail));
    }

    #[test]
    fn test_check_conditions_unknown_op() {
        let cond = ConditionSpec {
            field: "x".to_string(),
            op: "bogus".to_string(),
            value: "y".to_string(),
        };
        let fields = serde_json::json!({ "x": "y" });
        assert!(!check_conditions(&[cond], &fields));
    }

    #[test]
    fn test_check_conditions_empty() {
        let fields = serde_json::json!({ "x": "y" });
        assert!(check_conditions(&[], &fields));
    }

    #[test]
    fn test_update_rules_valid() {
        let worker = EventWorker::new();
        let rules = vec![serde_json::json!({
            "event": "test",
            "conditions": [],
            "action": { "type": "log" }
        })];
        worker.update_rules(rules);
        assert_eq!(worker.rules.lock().len(), 1);
    }

    #[test]
    fn test_update_rules_skips_invalid() {
        let worker = EventWorker::new();
        let rules = vec![
            serde_json::json!({
                "event": "test",
                "conditions": [],
                "action": { "type": "log" }
            }),
            serde_json::json!({
                "bad": "rule"
            }),
        ];
        worker.update_rules(rules);
        assert_eq!(worker.rules.lock().len(), 1);
    }

    #[test]
    fn test_update_rules_replaces_previous() {
        let worker = EventWorker::new();
        let rules1 = vec![serde_json::json!({
            "event": "a",
            "conditions": [],
            "action": { "type": "log" }
        })];
        worker.update_rules(rules1);
        assert_eq!(worker.rules.lock().len(), 1);

        let rules2 = vec![
            serde_json::json!({
                "event": "b",
                "conditions": [],
                "action": { "type": "log" }
            }),
            serde_json::json!({
                "event": "c",
                "conditions": [],
                "action": { "type": "log" }
            }),
        ];
        worker.update_rules(rules2);
        assert_eq!(worker.rules.lock().len(), 2);
    }

    #[test]
    fn test_execute_action_log() {
        let fields = serde_json::json!({ "key": "value" });
        let status = execute_action(&ActionSpec::Log, &fields);
        assert_eq!(status, "logged");
    }

    #[test]
    fn test_execute_action_webhook_skipped() {
        let fields = serde_json::json!({});
        let status = execute_action(
            &ActionSpec::Webhook {
                url: "https://example.com".to_string(),
            },
            &fields,
        );
        assert_eq!(status, "ok (skipped - no http client)");
    }

    #[test]
    fn test_execute_action_run() {
        let fields = serde_json::json!({});
        let status = execute_action(
            &ActionSpec::Run {
                command: "echo hello".to_string(),
            },
            &fields,
        );
        assert_eq!(status, "exit:0");
    }

    #[test]
    fn test_execute_action_run_with_interpolation() {
        let fields = serde_json::json!({ "name": "world" });
        let status = execute_action(
            &ActionSpec::Run {
                command: "echo ${name}".to_string(),
            },
            &fields,
        );
        assert_eq!(status, "exit:0");
    }

    #[test]
    fn test_deserialize_action_variants() {
        let notify: ActionSpec =
            serde_json::from_str(r#"{"type":"notify","message":"hello"}"#).unwrap();
        assert!(matches!(notify, ActionSpec::Notify { .. }));

        let log: ActionSpec = serde_json::from_str(r#"{"type":"log"}"#).unwrap();
        assert!(matches!(log, ActionSpec::Log));

        let run: ActionSpec = serde_json::from_str(r#"{"type":"run","command":"ls"}"#).unwrap();
        assert!(matches!(run, ActionSpec::Run { .. }));

        let webhook: ActionSpec =
            serde_json::from_str(r#"{"type":"webhook","url":"https://x.com"}"#).unwrap();
        assert!(matches!(webhook, ActionSpec::Webhook { .. }));

        let refresh: ActionSpec =
            serde_json::from_str(r#"{"type":"refresh","plugin":"git"}"#).unwrap();
        assert!(matches!(refresh, ActionSpec::Refresh { .. }));

        let pa: ActionSpec = serde_json::from_str(
            r#"{"type":"plugin_action","plugin":"git","action":"pull","params":{}}"#,
        )
        .unwrap();
        assert!(matches!(pa, ActionSpec::PluginAction { .. }));
    }
}
