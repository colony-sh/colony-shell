//! MCP (Model Context Protocol) servers sidebar panel

use crate::component::{
    Color, ComponentLine, SidebarComponent, SidebarOutput, Style, StyledSpan, TuplePattern,
};
use crate::context::ComponentContext;
use colony_tuple::Value;

/// MCP server status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpServerStatus {
    Connected,
    Connecting,
    Disconnected,
    Error(String),
}

/// Information about an MCP server
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub status: McpServerStatus,
    pub tool_count: usize,
    pub resource_count: usize,
}

/// MCP servers sidebar panel
pub struct McpPanel {
    /// Connected MCP servers
    servers: Vec<McpServerInfo>,
}

impl McpPanel {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
        }
    }

    /// Update server list (called by MCP connection pool)
    pub fn update_servers(&mut self, servers: Vec<McpServerInfo>) {
        self.servers = servers;
    }

    /// Add or update a server
    pub fn update_server(&mut self, info: McpServerInfo) {
        if let Some(existing) = self.servers.iter_mut().find(|s| s.name == info.name) {
            *existing = info;
        } else {
            self.servers.push(info);
        }
    }

    /// Remove a server
    pub fn remove_server(&mut self, name: &str) {
        self.servers.retain(|s| s.name != name);
    }
}

impl Default for McpPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarComponent for McpPanel {
    fn id(&self) -> &str {
        "mcp"
    }

    fn title(&self) -> &str {
        "MCP Servers"
    }

    fn default_width(&self) -> u16 {
        28
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![
            vec![
                Value::Str("mcp_status".to_string()),
                Value::Wildcard,
                Value::Wildcard,
            ],
            vec![Value::Str("mcp_connect".to_string()), Value::Wildcard],
            vec![Value::Str("mcp_disconnect".to_string()), Value::Wildcard],
        ]
    }

    fn render_block(&self, _ctx: &ComponentContext) -> SidebarOutput {
        let mut lines = Vec::new();

        if self.servers.is_empty() {
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "No servers connected",
                Style::default().dim().italic(),
            )]));
            lines.push(ComponentLine::plain(""));
            lines.push(ComponentLine::new(vec![StyledSpan::new(
                "Configure in .colonyrc",
                Style::default().dim(),
            )]));
        } else {
            for server in &self.servers {
                // Server name with status icon
                let (icon, status_style) = match &server.status {
                    McpServerStatus::Connected => (" ", Style::default().fg(Color::Green)),
                    McpServerStatus::Connecting => (" ", Style::default().fg(Color::Yellow)),
                    McpServerStatus::Disconnected => (" ", Style::default().fg(Color::Red)),
                    McpServerStatus::Error(_) => (" ", Style::default().fg(Color::Red)),
                };

                lines.push(
                    ComponentLine::new(vec![StyledSpan::new(server.name.clone(), status_style)])
                        .with_icon(icon)
                        .expandable(true),
                );

                // Tool count
                if server.tool_count > 0 {
                    lines.push(
                        ComponentLine::new(vec![
                            StyledSpan::new("Tools: ", Style::default().dim()),
                            StyledSpan::new(
                                server.tool_count.to_string(),
                                Style::default().fg(Color::Cyan),
                            ),
                        ])
                        .with_indent(1),
                    );
                }

                // Resource count
                if server.resource_count > 0 {
                    lines.push(
                        ComponentLine::new(vec![
                            StyledSpan::new("Resources: ", Style::default().dim()),
                            StyledSpan::new(
                                server.resource_count.to_string(),
                                Style::default().fg(Color::Magenta),
                            ),
                        ])
                        .with_indent(1),
                    );
                }

                // Error message if any
                if let McpServerStatus::Error(msg) = &server.status {
                    lines.push(
                        ComponentLine::new(vec![StyledSpan::new(
                            msg.clone(),
                            Style::default().fg(Color::Red).italic(),
                        )])
                        .with_indent(1),
                    );
                }
            }
        }

        SidebarOutput::visible(lines)
    }

    fn handle_action(&self, action: &str, _ctx: &ComponentContext) {
        if let Some(server) = action.strip_prefix("reconnect:") {
            tracing::debug!("MCP panel action: reconnect {}", server);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
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
    fn test_mcp_panel_empty() {
        let panel = McpPanel::new();
        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert!(!output.lines.is_empty());
    }

    #[test]
    fn test_mcp_panel_with_servers() {
        let mut panel = McpPanel::new();
        panel.update_server(McpServerInfo {
            name: "filesystem".to_string(),
            status: McpServerStatus::Connected,
            tool_count: 5,
            resource_count: 0,
        });

        let ctx = test_context();
        let output = panel.render_block(&ctx);
        assert!(output.visible);
        assert!(output.lines.len() >= 2);
    }
}
