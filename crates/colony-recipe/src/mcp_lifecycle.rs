//! MCP server lifecycle management
//!
//! Manages the lifecycle of MCP servers defined in recipe configurations.
//! Handles parsing, starting, stopping, and health checking of managed servers.

use std::collections::HashMap;

use crate::error::{RecipeError, Result};
use crate::recipe::McpServerConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    Stopped,
    Starting,
    Running,
    Unhealthy,
    Failed(String),
}

impl std::fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => write!(f, "stopped"),
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Failed(reason) => write!(f, "failed: {reason}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub server_name: String,
}

#[derive(Debug)]
pub struct ManagedServer {
    pub config: McpServerConfig,
    pub status: ServerStatus,
    pub tools: Vec<McpToolInfo>,
    pub pid: Option<u32>,
}

impl ManagedServer {
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            status: ServerStatus::Stopped,
            tools: Vec::new(),
            pid: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.status == ServerStatus::Running
    }

    pub fn mark_started(&mut self, pid: u32) {
        self.pid = Some(pid);
        self.status = ServerStatus::Running;
    }

    pub fn mark_failed(&mut self, reason: String) {
        self.status = ServerStatus::Failed(reason);
        self.pid = None;
    }

    pub fn mark_stopped(&mut self) {
        self.status = ServerStatus::Stopped;
        self.pid = None;
        self.tools.clear();
    }

    pub fn mark_unhealthy(&mut self) {
        self.status = ServerStatus::Unhealthy;
    }

    pub fn register_tools(&mut self, tools: Vec<McpToolInfo>) {
        self.tools = tools;
    }
}

pub struct McpLifecycleManager {
    servers: HashMap<String, ManagedServer>,
}

impl Default for McpLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpLifecycleManager {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    pub fn from_configs(configs: &[McpServerConfig]) -> Self {
        let mut manager = Self::new();
        for config in configs {
            manager.add_server(config.clone());
        }
        manager
    }

    pub fn add_server(&mut self, config: McpServerConfig) {
        let name = config.name.clone();
        self.servers.insert(name, ManagedServer::new(config));
    }

    pub fn remove_server(&mut self, name: &str) -> Option<ManagedServer> {
        self.servers.remove(name)
    }

    pub fn get_server(&self, name: &str) -> Option<&ManagedServer> {
        self.servers.get(name)
    }

    pub fn get_server_mut(&mut self, name: &str) -> Option<&mut ManagedServer> {
        self.servers.get_mut(name)
    }

    pub fn server_names(&self) -> Vec<&str> {
        self.servers.keys().map(|s| s.as_str()).collect()
    }

    pub fn all_servers(&self) -> impl Iterator<Item = &ManagedServer> {
        self.servers.values()
    }

    pub fn running_servers(&self) -> Vec<&ManagedServer> {
        self.servers.values().filter(|s| s.is_running()).collect()
    }

    pub fn available_tools(&self) -> Vec<&McpToolInfo> {
        self.servers
            .values()
            .filter(|s| s.is_running())
            .flat_map(|s| &s.tools)
            .collect()
    }

    pub fn find_tool(&self, tool_name: &str) -> Option<(&ManagedServer, &McpToolInfo)> {
        for server in self.servers.values() {
            if !server.is_running() {
                continue;
            }
            for tool in &server.tools {
                if tool.name == tool_name {
                    return Some((server, tool));
                }
            }
        }
        None
    }

    pub fn start_server(&mut self, name: &str, pid: u32) -> Result<()> {
        let server = self
            .servers
            .get_mut(name)
            .ok_or_else(|| RecipeError::NotFound(format!("MCP server '{name}'")))?;

        server.mark_started(pid);
        Ok(())
    }

    pub fn stop_server(&mut self, name: &str) -> Result<()> {
        let server = self
            .servers
            .get_mut(name)
            .ok_or_else(|| RecipeError::NotFound(format!("MCP server '{name}'")))?;

        server.mark_stopped();
        Ok(())
    }

    pub fn stop_all(&mut self) {
        for server in self.servers.values_mut() {
            server.mark_stopped();
        }
    }

    pub fn health_check(&mut self, name: &str, healthy: bool) -> Result<()> {
        let server = self
            .servers
            .get_mut(name)
            .ok_or_else(|| RecipeError::NotFound(format!("MCP server '{name}'")))?;

        if !healthy && server.is_running() {
            server.mark_unhealthy();
        } else if healthy && server.status == ServerStatus::Unhealthy && server.pid.is_some() {
            server.status = ServerStatus::Running;
        }

        Ok(())
    }

    pub fn status_summary(&self) -> HashMap<&str, &ServerStatus> {
        self.servers
            .iter()
            .map(|(name, server)| (name.as_str(), &server.status))
            .collect()
    }
}

pub fn validate_server_config(config: &McpServerConfig) -> Result<()> {
    if config.name.is_empty() {
        return Err(RecipeError::Invalid(
            "MCP server name cannot be empty".to_string(),
        ));
    }

    if config.command.is_empty() {
        return Err(RecipeError::Invalid(format!(
            "MCP server '{}' has no command",
            config.name
        )));
    }

    Ok(())
}

pub fn build_server_command(
    config: &McpServerConfig,
) -> (String, Vec<String>, HashMap<String, String>) {
    (
        config.command.clone(),
        config.args.clone(),
        config.env.clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), format!("@test/{name}")],
            env: HashMap::new(),
        }
    }

    #[test]
    fn test_managed_server_lifecycle() {
        let config = test_config("filesystem");
        let mut server = ManagedServer::new(config);

        assert_eq!(server.status, ServerStatus::Stopped);
        assert!(!server.is_running());
        assert!(server.pid.is_none());

        server.mark_started(12345);
        assert_eq!(server.status, ServerStatus::Running);
        assert!(server.is_running());
        assert_eq!(server.pid, Some(12345));

        server.mark_unhealthy();
        assert_eq!(server.status, ServerStatus::Unhealthy);
        assert!(!server.is_running());

        server.mark_stopped();
        assert_eq!(server.status, ServerStatus::Stopped);
        assert!(server.pid.is_none());
    }

    #[test]
    fn test_managed_server_failed() {
        let config = test_config("broken");
        let mut server = ManagedServer::new(config);

        server.mark_failed("connection refused".to_string());
        assert!(matches!(server.status, ServerStatus::Failed(_)));
        assert!(server.pid.is_none());
    }

    #[test]
    fn test_managed_server_tools() {
        let config = test_config("filesystem");
        let mut server = ManagedServer::new(config);
        server.mark_started(1);

        server.register_tools(vec![
            McpToolInfo {
                name: "read_file".to_string(),
                description: Some("Read a file".to_string()),
                server_name: "filesystem".to_string(),
            },
            McpToolInfo {
                name: "write_file".to_string(),
                description: None,
                server_name: "filesystem".to_string(),
            },
        ]);

        assert_eq!(server.tools.len(), 2);

        server.mark_stopped();
        assert!(server.tools.is_empty());
    }

    #[test]
    fn test_lifecycle_manager_from_configs() {
        let configs = vec![test_config("fs"), test_config("github")];
        let manager = McpLifecycleManager::from_configs(&configs);

        let mut names = manager.server_names();
        names.sort();
        assert_eq!(names, vec!["fs", "github"]);
    }

    #[test]
    fn test_lifecycle_manager_start_stop() {
        let mut manager = McpLifecycleManager::new();
        manager.add_server(test_config("fs"));

        assert!(manager.running_servers().is_empty());

        manager.start_server("fs", 100).unwrap();
        assert_eq!(manager.running_servers().len(), 1);
        assert!(manager.get_server("fs").unwrap().is_running());

        manager.stop_server("fs").unwrap();
        assert!(manager.running_servers().is_empty());
    }

    #[test]
    fn test_lifecycle_manager_stop_all() {
        let mut manager = McpLifecycleManager::new();
        manager.add_server(test_config("a"));
        manager.add_server(test_config("b"));

        manager.start_server("a", 1).unwrap();
        manager.start_server("b", 2).unwrap();
        assert_eq!(manager.running_servers().len(), 2);

        manager.stop_all();
        assert!(manager.running_servers().is_empty());
    }

    #[test]
    fn test_lifecycle_manager_available_tools() {
        let mut manager = McpLifecycleManager::new();
        manager.add_server(test_config("fs"));
        manager.start_server("fs", 1).unwrap();

        let server = manager.get_server_mut("fs").unwrap();
        server.register_tools(vec![McpToolInfo {
            name: "read_file".to_string(),
            description: None,
            server_name: "fs".to_string(),
        }]);

        let tools = manager.available_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "read_file");
    }

    #[test]
    fn test_lifecycle_manager_find_tool() {
        let mut manager = McpLifecycleManager::new();
        manager.add_server(test_config("fs"));
        manager.start_server("fs", 1).unwrap();

        let server = manager.get_server_mut("fs").unwrap();
        server.register_tools(vec![McpToolInfo {
            name: "read_file".to_string(),
            description: None,
            server_name: "fs".to_string(),
        }]);

        let found = manager.find_tool("read_file");
        assert!(found.is_some());
        let (srv, tool) = found.unwrap();
        assert_eq!(srv.config.name, "fs");
        assert_eq!(tool.name, "read_file");

        assert!(manager.find_tool("nonexistent").is_none());
    }

    #[test]
    fn test_lifecycle_manager_not_found() {
        let mut manager = McpLifecycleManager::new();
        assert!(manager.start_server("nope", 1).is_err());
        assert!(manager.stop_server("nope").is_err());
    }

    #[test]
    fn test_health_check() {
        let mut manager = McpLifecycleManager::new();
        manager.add_server(test_config("fs"));
        manager.start_server("fs", 1).unwrap();

        manager.health_check("fs", false).unwrap();
        assert_eq!(
            manager.get_server("fs").unwrap().status,
            ServerStatus::Unhealthy
        );

        manager.health_check("fs", true).unwrap();
        assert_eq!(
            manager.get_server("fs").unwrap().status,
            ServerStatus::Running
        );
    }

    #[test]
    fn test_validate_server_config_valid() {
        let config = test_config("fs");
        assert!(validate_server_config(&config).is_ok());
    }

    #[test]
    fn test_validate_server_config_empty_name() {
        let config = McpServerConfig {
            name: String::new(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
        };
        assert!(validate_server_config(&config).is_err());
    }

    #[test]
    fn test_validate_server_config_empty_command() {
        let config = McpServerConfig {
            name: "test".to_string(),
            command: String::new(),
            args: vec![],
            env: HashMap::new(),
        };
        assert!(validate_server_config(&config).is_err());
    }

    #[test]
    fn test_build_server_command() {
        let mut env = HashMap::new();
        env.insert("TOKEN".to_string(), "secret".to_string());
        let config = McpServerConfig {
            name: "github".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@mcp/github".to_string()],
            env,
        };

        let (cmd, args, env) = build_server_command(&config);
        assert_eq!(cmd, "npx");
        assert_eq!(args, vec!["-y", "@mcp/github"]);
        assert_eq!(env.get("TOKEN").unwrap(), "secret");
    }

    #[test]
    fn test_status_summary() {
        let mut manager = McpLifecycleManager::new();
        manager.add_server(test_config("a"));
        manager.add_server(test_config("b"));
        manager.start_server("a", 1).unwrap();

        let summary = manager.status_summary();
        assert_eq!(*summary.get("a").unwrap(), &ServerStatus::Running);
        assert_eq!(*summary.get("b").unwrap(), &ServerStatus::Stopped);
    }

    #[test]
    fn test_remove_server() {
        let mut manager = McpLifecycleManager::new();
        manager.add_server(test_config("fs"));
        assert!(manager.get_server("fs").is_some());

        let removed = manager.remove_server("fs");
        assert!(removed.is_some());
        assert!(manager.get_server("fs").is_none());
    }

    #[test]
    fn test_server_status_display() {
        assert_eq!(ServerStatus::Stopped.to_string(), "stopped");
        assert_eq!(ServerStatus::Starting.to_string(), "starting");
        assert_eq!(ServerStatus::Running.to_string(), "running");
        assert_eq!(ServerStatus::Unhealthy.to_string(), "unhealthy");
        assert_eq!(
            ServerStatus::Failed("timeout".to_string()).to_string(),
            "failed: timeout"
        );
    }
}
