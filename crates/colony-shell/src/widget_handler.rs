//! Widget message handler for interactive prompt components
//!
//! This module handles OSC 2337 messages from colony-terminal for widget interactions.
//!
//! Integration: Colony-terminal should send WidgetRequest/WidgetAction messages via
//! stdin OSC sequences. The shell parses these and calls the appropriate handler.
//!
//! Currently, this requires a separate stdin reader that can detect OSC sequences
//! and route them here. This could be done via:
//! - A separate thread monitoring stdin for OSC sequences
//! - Colony-terminal using a Unix socket for widget IPC
//! - PTY proxying in colony-terminal that intercepts OSC responses

use colony_protocol::{
    emit_widget_data, emit_widget_result, parse_widget_action, parse_widget_request,
};
use colony_ui_core::{checkout_remote, create_branch, gather_branch_data, switch_branch};
use std::path::Path;

pub fn handle_widget_request(json: &str, cwd: &Path) -> bool {
    if let Some((widget, _component_id, _context)) = parse_widget_request(json) {
        if widget.as_str() == "branch_picker" {
            if let Some(data) = gather_branch_data(cwd) {
                emit_widget_data("branch_picker", serde_json::to_value(data).unwrap());
            } else {
                emit_widget_result(
                    "branch_picker",
                    false,
                    None,
                    Some("Not in a git repository"),
                );
            }
            return true;
        }
    }
    false
}

pub fn handle_widget_action(json: &str, cwd: &Path) -> bool {
    if let Some((widget, action, payload)) = parse_widget_action(json) {
        match (widget.as_str(), action.as_str()) {
            ("branch_picker", "switch") => {
                if let Some(branch) = payload.get("branch").and_then(|v| v.as_str()) {
                    match switch_branch(cwd, branch) {
                        Ok(msg) => emit_widget_result("branch_picker", true, Some(&msg), None),
                        Err(err) => emit_widget_result("branch_picker", false, None, Some(&err)),
                    }
                }
                return true;
            }
            ("branch_picker", "create") => {
                let name = payload.get("name").and_then(|v| v.as_str());
                let from = payload.get("from").and_then(|v| v.as_str());

                if let (Some(name), Some(from)) = (name, from) {
                    match create_branch(cwd, name, from) {
                        Ok(msg) => emit_widget_result("branch_picker", true, Some(&msg), None),
                        Err(err) => emit_widget_result("branch_picker", false, None, Some(&err)),
                    }
                }
                return true;
            }
            ("branch_picker", "checkout_remote") => {
                let remote = payload.get("remote").and_then(|v| v.as_str());
                let local_name = payload.get("local_name").and_then(|v| v.as_str());

                if let (Some(remote), Some(local_name)) = (remote, local_name) {
                    match checkout_remote(cwd, remote, local_name) {
                        Ok(msg) => emit_widget_result("branch_picker", true, Some(&msg), None),
                        Err(err) => emit_widget_result("branch_picker", false, None, Some(&err)),
                    }
                }
                return true;
            }
            _ => {}
        }
    }
    false
}

#[allow(dead_code)]
pub fn try_handle_osc_message(json: &str, cwd: &Path) -> bool {
    handle_widget_request(json, cwd) || handle_widget_action(json, cwd)
}
