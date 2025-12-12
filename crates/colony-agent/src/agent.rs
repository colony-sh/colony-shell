//! Agent orchestration and Claude API integration

use crate::{tool::{ToolRegistry, ToolClassification}, Result, AgentError};
use claude_sdk::{ClaudeClient, ConversationBuilder, StreamEvent, models};
use futures::StreamExt;
use tracing::{debug, info, warn};

/// Colony Shell agent powered by Claude
pub struct Agent {
    /// Claude API client
    client: ClaudeClient,

    /// Current conversation state
    conversation: ConversationBuilder,

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
    /// Claude model to use
    pub model: String,

    /// Max tokens per response
    pub max_tokens: u32,

    /// System prompt
    pub system_prompt: String,

    /// Enable prompt caching
    pub use_caching: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: models::CLAUDE_SONNET_4_5.anthropic_id.to_string(),
            max_tokens: 4096,
            system_prompt: "You are Colony Shell's agent assistant.".to_string(),
            use_caching: true,
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
        std::io::stdout().flush().map_err(|e| {
            AgentError::Protocol(format!("failed to flush stdout: {}", e))
        })?;
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
        warn!("Tool approval not yet implemented, auto-approving: {}", tool_name);
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
    /// Create new agent with API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_config(api_key, AgentConfig::default())
    }

    /// Create agent with custom configuration
    pub fn with_config(api_key: impl Into<String>, config: AgentConfig) -> Self {
        let client = ClaudeClient::anthropic(api_key);

        let conversation = if config.use_caching {
            ConversationBuilder::new()
                .with_cached_system(&config.system_prompt)
        } else {
            ConversationBuilder::new()
                .with_system(&config.system_prompt)
        };

        Self {
            client,
            conversation,
            tools: ToolRegistry::new(),
            protocol: ProtocolEmitter::new(),
            config,
        }
    }

    /// Create agent for AWS Bedrock (requires --features bedrock)
    #[cfg(feature = "bedrock")]
    pub async fn bedrock(region: impl Into<String>, config: AgentConfig) -> Result<Self> {
        let client = ClaudeClient::bedrock(region)
            .await
            .map_err(AgentError::ClaudeApi)?;

        let conversation = if config.use_caching {
            ConversationBuilder::new()
                .with_cached_system(&config.system_prompt)
        } else {
            ConversationBuilder::new()
                .with_system(&config.system_prompt)
        };

        Ok(Self {
            client,
            conversation,
            tools: ToolRegistry::new(),
            protocol: ProtocolEmitter::new(),
            config,
        })
    }

    /// Get mutable access to tool registry
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.tools
    }

    /// Process user input and return agent response
    pub async fn process(&mut self, user_input: impl Into<String>) -> Result<String> {
        let input = user_input.into();
        info!("Processing user input: {}", input);

        // Add user message to conversation
        self.conversation.add_user_message(&input);

        // Build request with tools
        let tools = self.tools.definitions();
        let builder = self.conversation.clone();
        let request = builder
            .with_tools(tools)
            .build(&self.config.model, self.config.max_tokens);

        // Send streaming request
        let mut stream = self.client.send_streaming(request).await?;
        let mut response_text = String::new();

        // Process stream events
        while let Some(event) = stream.next().await {
            match event? {
                StreamEvent::ContentBlockDelta { delta, .. } => {
                    if let Some(text) = delta.text() {
                        response_text.push_str(text);
                        self.protocol.emit_thought(text)?;
                    }
                }

                StreamEvent::ContentBlockStart { content_block, .. } => {
                    // Handle tool use
                    if let claude_sdk::ContentBlock::ToolUse { id, name, input, .. } = content_block {
                        self.handle_tool_use(id, name, input).await?;
                    }
                }

                StreamEvent::MessageStop => {
                    debug!("Stream completed");
                    break;
                }

                _ => {}
            }
        }

        // Add assistant response to conversation
        self.conversation.add_assistant_message(&response_text);

        Ok(response_text)
    }

    /// Handle tool use from Claude
    async fn handle_tool_use(
        &mut self,
        tool_use_id: String,
        tool_name: String,
        input: serde_json::Value,
    ) -> Result<()> {
        info!("Tool use: {} (id: {})", tool_name, tool_use_id);

        // Check classification
        let classification = self.tools.classification(&tool_name)
            .ok_or_else(|| AgentError::ToolExecution(
                format!("unknown tool: {}", tool_name)
            ))?;

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
                    ApprovalResponse::Approved => {
                        self.execute_tool(&tool_name, input).await?
                    }
                    ApprovalResponse::Modified(modified_input) => {
                        self.execute_tool(&tool_name, modified_input).await?
                    }
                    ApprovalResponse::Denied(reason) => {
                        return Err(AgentError::ToolExecution(
                            format!("user denied: {}", reason)
                        ));
                    }
                }
            }
        };

        // Add tool result to conversation (SDK expects JSON string)
        let result_str = serde_json::to_string(&result)
            .map_err(|e| AgentError::Conversation(format!("failed to serialize tool result: {}", e)))?;

        self.conversation.add_tool_result(tool_use_id, result_str);

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
