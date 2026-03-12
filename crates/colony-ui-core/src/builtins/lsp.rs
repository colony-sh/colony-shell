use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use parking_lot::Mutex;

/// LSP server connection status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspStatus {
    Running,
    Starting,
    Stopped,
    Error(String),
}

/// Information about a connected LSP server
#[derive(Debug, Clone)]
pub struct LspServerInfo {
    pub language: String,
    pub server_name: String,
    pub status: LspStatus,
}

pub struct LspComponent {
    server_info: Mutex<Option<LspServerInfo>>,
}

impl LspComponent {
    pub fn new() -> Self {
        Self {
            server_info: Mutex::new(None),
        }
    }

    /// Update the LSP server info
    pub fn set_server_info(&self, info: Option<LspServerInfo>) {
        *self.server_info.lock() = info;
    }
}

impl Default for LspComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for LspComponent {
    fn id(&self) -> &str {
        "lsp"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("lsp_status".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]]
    }

    fn render(&self, _ctx: &ComponentContext) -> ComponentOutput {
        let info = self.server_info.lock();

        let Some(ref server) = *info else {
            return ComponentOutput::hidden();
        };

        let (icon, color) = match server.status {
            LspStatus::Running => ("\u{f085}", Color::Green),   // cog icon
            LspStatus::Starting => ("\u{f110}", Color::Yellow), // spinner
            LspStatus::Stopped => return ComponentOutput::hidden(),
            LspStatus::Error(_) => ("\u{f071}", Color::Red),    // warning
        };

        let text = format!("{} {} [{}]", icon, server.server_name, server.language);
        let spans = vec![StyledSpan::new(text, Style::default().fg(color))];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "language": server.language,
                "server": server.server_name,
                "status": format!("{:?}", server.status),
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            ..Default::default()
        }
    }

    #[test]
    fn test_lsp_id() {
        let component = LspComponent::new();
        assert_eq!(component.id(), "lsp");
    }

    #[test]
    fn test_lsp_no_server_hidden() {
        let component = LspComponent::new();
        let ctx = test_context();
        let output = component.render(&ctx);
        assert!(!output.visible);
    }

    #[test]
    fn test_lsp_running_visible() {
        let component = LspComponent::new();
        component.set_server_info(Some(LspServerInfo {
            language: "rust".to_string(),
            server_name: "rust-analyzer".to_string(),
            status: LspStatus::Running,
        }));

        let ctx = test_context();
        let output = component.render(&ctx);

        assert!(output.visible);
        assert!(output.spans[0].text.contains("rust-analyzer"));
        assert!(output.spans[0].text.contains("rust"));
    }

    #[test]
    fn test_lsp_starting_visible() {
        let component = LspComponent::new();
        component.set_server_info(Some(LspServerInfo {
            language: "python".to_string(),
            server_name: "pyright".to_string(),
            status: LspStatus::Starting,
        }));

        let ctx = test_context();
        let output = component.render(&ctx);

        assert!(output.visible);
        assert!(output.spans[0].text.contains("pyright"));
    }

    #[test]
    fn test_lsp_stopped_hidden() {
        let component = LspComponent::new();
        component.set_server_info(Some(LspServerInfo {
            language: "rust".to_string(),
            server_name: "rust-analyzer".to_string(),
            status: LspStatus::Stopped,
        }));

        let ctx = test_context();
        let output = component.render(&ctx);
        assert!(!output.visible);
    }

    #[test]
    fn test_lsp_error_visible() {
        let component = LspComponent::new();
        component.set_server_info(Some(LspServerInfo {
            language: "go".to_string(),
            server_name: "gopls".to_string(),
            status: LspStatus::Error("connection lost".to_string()),
        }));

        let ctx = test_context();
        let output = component.render(&ctx);

        assert!(output.visible);
        assert!(output.spans[0].text.contains("gopls"));
    }

    #[test]
    fn test_lsp_set_clear() {
        let component = LspComponent::new();

        component.set_server_info(Some(LspServerInfo {
            language: "rust".to_string(),
            server_name: "ra".to_string(),
            status: LspStatus::Running,
        }));
        assert!(component.render(&test_context()).visible);

        component.set_server_info(None);
        assert!(!component.render(&test_context()).visible);
    }

    #[test]
    fn test_lsp_subscriptions() {
        let component = LspComponent::new();
        let subs = component.subscriptions();
        assert_eq!(subs.len(), 1);
    }
}
