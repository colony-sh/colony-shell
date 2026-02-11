use crate::component::{
    SidebarComponent, SidebarLine, SidebarOutput, SidebarPosition, TuplePattern,
};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

/// Represents a connected MCP server and its tools
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub status: McpServerStatus,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpServerStatus {
    Connected,
    Connecting,
    Disconnected,
    Error,
}

pub struct McpSidebarComponent {
    servers: parking_lot::Mutex<Vec<McpServerInfo>>,
}

impl McpSidebarComponent {
    pub fn new() -> Self {
        Self {
            servers: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// Update the list of MCP servers
    pub fn set_servers(&self, servers: Vec<McpServerInfo>) {
        *self.servers.lock() = servers;
    }

    /// Add or update a single server
    pub fn upsert_server(&self, server: McpServerInfo) {
        let mut servers = self.servers.lock();
        if let Some(existing) = servers.iter_mut().find(|s| s.name == server.name) {
            *existing = server;
        } else {
            servers.push(server);
        }
    }
}

impl Default for McpSidebarComponent {
    fn default() -> Self {
        Self::new()
    }
}

fn status_indicator(status: McpServerStatus) -> (&'static str, Color) {
    match status {
        McpServerStatus::Connected => ("\u{25cf}", Color::Green),     // filled circle
        McpServerStatus::Connecting => ("\u{25cb}", Color::Yellow),   // open circle
        McpServerStatus::Disconnected => ("\u{25cb}", Color::BrightBlack), // dim open circle
        McpServerStatus::Error => ("\u{25cf}", Color::Red),           // red filled circle
    }
}

impl SidebarComponent for McpSidebarComponent {
    fn id(&self) -> &str {
        "mcp"
    }

    fn title(&self) -> &str {
        "MCP Servers"
    }

    fn position(&self) -> SidebarPosition {
        SidebarPosition::Right
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("mcp_status".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]]
    }

    fn render(&self, _ctx: &ComponentContext) -> SidebarOutput {
        let servers = self.servers.lock();

        if servers.is_empty() {
            return SidebarOutput::visible(vec![SidebarLine::new(StyledSpan::new(
                "No servers connected",
                Style::default().fg(Color::BrightBlack).italic(),
            ))]);
        }

        let mut lines = Vec::new();

        for server in servers.iter() {
            let (icon, color) = status_indicator(server.status);

            // Server name line
            lines.push(
                SidebarLine::new(StyledSpan::new(
                    server.name.clone(),
                    Style::default().fg(Color::White).bold(),
                ))
                .with_icon(StyledSpan::new(icon, Style::default().fg(color)))
                .with_metadata(StyledSpan::new(
                    format!("{} tools", server.tools.len()),
                    Style::default().fg(Color::BrightBlack),
                )),
            );

            // Tool lines (indented)
            for tool in &server.tools {
                lines.push(
                    SidebarLine::new(StyledSpan::new(
                        tool.clone(),
                        Style::default().fg(Color::BrightBlack),
                    ))
                    .with_icon(StyledSpan::new(
                        "\u{f0ad}",
                        Style::default().fg(Color::BrightBlack),
                    ))
                    .with_indent(1),
                );
            }
        }

        let server_data: Vec<serde_json::Value> = servers
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "status": format!("{:?}", s.status),
                    "tool_count": s.tools.len(),
                })
            })
            .collect();

        SidebarOutput::with_data(lines, serde_json::json!({ "servers": server_data }))
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
    fn test_mcp_id() {
        let component = McpSidebarComponent::new();
        assert_eq!(component.id(), "mcp");
    }

    #[test]
    fn test_mcp_title() {
        let component = McpSidebarComponent::new();
        assert_eq!(component.title(), "MCP Servers");
    }

    #[test]
    fn test_mcp_empty_servers() {
        let component = McpSidebarComponent::new();
        let ctx = test_context();
        let output = component.render(&ctx);

        assert!(output.visible);
        assert_eq!(output.lines.len(), 1);
        assert!(output.lines[0].text.text.contains("No servers"));
    }

    #[test]
    fn test_mcp_with_servers() {
        let component = McpSidebarComponent::new();
        component.set_servers(vec![
            McpServerInfo {
                name: "filesystem".to_string(),
                status: McpServerStatus::Connected,
                tools: vec!["read_file".to_string(), "write_file".to_string()],
            },
            McpServerInfo {
                name: "git".to_string(),
                status: McpServerStatus::Connected,
                tools: vec!["commit".to_string()],
            },
        ]);

        let ctx = test_context();
        let output = component.render(&ctx);

        assert!(output.visible);
        // 2 server header lines + 2 tools for first + 1 tool for second = 5 lines
        assert_eq!(output.lines.len(), 5);
    }

    #[test]
    fn test_mcp_upsert_server() {
        let component = McpSidebarComponent::new();
        component.upsert_server(McpServerInfo {
            name: "test".to_string(),
            status: McpServerStatus::Connected,
            tools: vec![],
        });

        // Update same server
        component.upsert_server(McpServerInfo {
            name: "test".to_string(),
            status: McpServerStatus::Error,
            tools: vec!["tool1".to_string()],
        });

        let servers = component.servers.lock();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].status, McpServerStatus::Error);
        assert_eq!(servers[0].tools.len(), 1);
    }

    #[test]
    fn test_status_indicator_colors() {
        let (_, color) = status_indicator(McpServerStatus::Connected);
        assert_eq!(color, Color::Green);

        let (_, color) = status_indicator(McpServerStatus::Error);
        assert_eq!(color, Color::Red);

        let (_, color) = status_indicator(McpServerStatus::Connecting);
        assert_eq!(color, Color::Yellow);
    }
}
