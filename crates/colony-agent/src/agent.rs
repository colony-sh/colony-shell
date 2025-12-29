//! Agent orchestration with multi-provider LLM support

use crate::{
    provider::{
        CompletionRequest, Message, MessageContent, MessageRole, ModelProvider, ToolDefinition,
    },
    router::ModelRouter,
    tool::{ToolClassification, ToolRegistry},
    AgentError, ProviderType, Result, StreamEvent,
};
use futures::StreamExt;
use tracing::{debug, info, warn};

/// Colony Shell agent with multi-provider support
pub struct Agent {
    /// Model router (handles provider selection)
    router: ModelRouter,

    /// Conversation history (provider-agnostic)
    messages: Vec<Message>,

    /// Tool registry with execution handlers
    tools: ToolRegistry,

    /// Protocol emitter for terminal communication
    protocol: ProtocolEmitter,

    /// Agent configuration
    config: AgentConfig,
}

/// Agent configuration
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Max tokens per response
    pub max_tokens: u32,

    /// System prompt
    pub system_prompt: String,

    /// Enable prompt caching (if provider supports it)
    pub use_caching: bool,

    /// Temperature (0.0-1.0)
    pub temperature: Option<f32>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            system_prompt: "You are Colony Shell's agent assistant.".to_string(),
            use_caching: true,
            temperature: None,
        }
    }
}

/// Protocol message emitter
pub struct ProtocolEmitter {
    // TODO: This will emit OSC 2337 sequences
    // For now, just print to stdout
}

impl ProtocolEmitter {
    pub fn new() -> Self {
        Self {}
    }

    /// Emit agent thought (streaming text)
    pub fn emit_thought(&self, text: &str) -> Result<()> {
        // TODO: Emit via OSC 2337
        print!("{}", text);
        use std::io::Write;
        std::io::stdout()
            .flush()
            .map_err(|e| AgentError::Protocol(format!("failed to flush stdout: {}", e)))?;
        Ok(())
    }

    /// Emit tool call notification
    pub fn emit_tool_call(&self, tool_name: &str, input: &serde_json::Value) -> Result<()> {
        debug!("Tool call: {} with input: {:?}", tool_name, input);
        // TODO: Emit structured message via OSC 2337
        Ok(())
    }

    /// Request tool approval from user
    pub async fn request_approval(
        &self,
        tool_name: &str,
        _input: &serde_json::Value,
    ) -> Result<ApprovalResponse> {
        // TODO: Emit approval request widget via OSC 2337
        // TODO: Wait for response from terminal via tuple space or IPC

        // For now, auto-approve (placeholder)
        warn!(
            "Tool approval not yet implemented, auto-approving: {}",
            tool_name
        );
        Ok(ApprovalResponse::Approved)
    }
}

/// Tool approval response from terminal
#[derive(Debug, Clone)]
pub enum ApprovalResponse {
    Approved,
    Modified(serde_json::Value),
    Denied(String),
}

impl Agent {
    /// Create new agent with auto-detected provider
    ///
    /// Detection order: Anthropic (if ANTHROPIC_API_KEY) → OpenAI (if OPENAI_API_KEY) → Ollama (local, free)
    pub async fn new() -> Result<Self> {
        Self::with_config(AgentConfig::default()).await
    }

    /// Create agent with custom configuration
    pub async fn with_config(config: AgentConfig) -> Result<Self> {
        let router = ModelRouter::auto_detect().await?;

        info!(
            "Agent initialized with provider: {}, model: {}",
            router.provider_name(),
            router.model_name()
        );

        Ok(Self {
            router,
            messages: Vec::new(),
            tools: ToolRegistry::new(),
            protocol: ProtocolEmitter::new(),
            config,
        })
    }

    /// Create agent with specific provider
    pub async fn with_provider(
        provider_type: ProviderType,
        model: impl Into<String>,
        config: AgentConfig,
    ) -> Result<Self> {
        let router = ModelRouter::from_config(crate::router::RouterConfig {
            provider_type,
            model_name: model.into(),
        })
        .await?;

        info!(
            "Agent initialized with explicit provider: {}, model: {}",
            router.provider_name(),
            router.model_name()
        );

        Ok(Self {
            router,
            messages: Vec::new(),
            tools: ToolRegistry::new(),
            protocol: ProtocolEmitter::new(),
            config,
        })
    }

    /// Switch to a different model provider at runtime
    pub async fn switch_model(
        &mut self,
        provider_type: ProviderType,
        model: impl Into<String>,
    ) -> Result<()> {
        let model_name = model.into();
        info!("Switching model to {:?}: {}", provider_type, model_name);

        self.router
            .switch_provider(crate::router::RouterConfig {
                provider_type,
                model_name,
            })
            .await
    }

    /// Get current provider info
    pub fn current_provider(&self) -> (&ProviderType, &str) {
        (self.router.current_provider(), self.router.current_model())
    }

    /// Get mutable access to tool registry
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.tools
    }

    /// Process user input and return agent response
    pub async fn process(&mut self, user_input: impl Into<String>) -> Result<String> {
        let input = user_input.into();
        info!("Processing user input: {}", input);

        // Add user message to conversation history
        self.messages.push(Message {
            role: MessageRole::User,
            content: MessageContent::Text(input),
        });

        // Build completion request
        let tool_defs = self.convert_tool_definitions();

        let mut request = CompletionRequest::new()
            .with_system(&self.config.system_prompt)
            .with_max_tokens(self.config.max_tokens)
            .with_tools(tool_defs);

        if self.config.use_caching {
            request = request.enable_caching();
        }

        // Add all messages to request
        request.messages = self.messages.clone();

        // Send streaming request via router
        let mut stream = self.router.complete(request).await?;
        let mut response_text = String::new();
        let mut pending_tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

        // Process stream events
        while let Some(event) = stream.next().await {
            match event? {
                StreamEvent::TextDelta { text, .. } => {
                    response_text.push_str(&text);
                    self.protocol.emit_thought(&text)?;
                }

                StreamEvent::ToolCallStart { id, name } => {
                    debug!("Tool call started: {} (id: {})", name, id);
                    // Store for later completion
                    pending_tool_calls.push((id, name, serde_json::Value::Null));
                }

                StreamEvent::ToolCallComplete { id, name, input } => {
                    info!("Tool call complete: {} (id: {})", name, id);
                    // Handle tool execution
                    self.handle_tool_use(id.clone(), name.clone(), input.clone())
                        .await?;
                }

                StreamEvent::Done => {
                    debug!("Stream completed");
                    break;
                }

                StreamEvent::Error { message } => {
                    return Err(AgentError::Protocol(format!("Stream error: {}", message)));
                }
            }
        }

        // Add assistant response to conversation history
        if !response_text.is_empty() {
            self.messages.push(Message {
                role: MessageRole::Assistant,
                content: MessageContent::Text(response_text.clone()),
            });
        }

        Ok(response_text)
    }

    /// Convert tool registry to provider-agnostic definitions
    fn convert_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .definitions()
            .iter()
            .map(|t| ToolDefinition {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect()
    }

    /// Handle tool use from LLM
    async fn handle_tool_use(
        &mut self,
        tool_use_id: String,
        tool_name: String,
        input: serde_json::Value,
    ) -> Result<()> {
        info!("Tool use: {} (id: {})", tool_name, tool_use_id);

        // Check classification
        let classification = self
            .tools
            .classification(&tool_name)
            .ok_or_else(|| AgentError::ToolExecution(format!("unknown tool: {}", tool_name)))?;

        // Execute based on classification
        let result = match classification {
            ToolClassification::Programmatic => {
                // Auto-execute
                self.protocol.emit_tool_call(&tool_name, &input)?;
                self.execute_tool(&tool_name, input).await?
            }

            ToolClassification::UserFacing => {
                // Request approval
                let approval = self.protocol.request_approval(&tool_name, &input).await?;

                match approval {
                    ApprovalResponse::Approved => self.execute_tool(&tool_name, input).await?,
                    ApprovalResponse::Modified(modified_input) => {
                        self.execute_tool(&tool_name, modified_input).await?
                    }
                    ApprovalResponse::Denied(reason) => {
                        return Err(AgentError::ToolExecution(format!(
                            "user denied: {}",
                            reason
                        )));
                    }
                }
            }
        };

        // Add tool result to message history
        let result_str = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());

        // Tool results go as user messages in next turn
        // (Provider-specific handling varies, keeping it simple)
        self.messages.push(Message {
            role: MessageRole::User,
            content: MessageContent::Text(format!("Tool result: {}", result_str)),
        });

        Ok(())
    }

    /// Execute tool and return result
    async fn execute_tool(
        &self,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.tools
            .execute(tool_name, input)
            .map_err(|e| AgentError::ToolExecution(e.to_string()))
    }
}
