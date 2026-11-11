//! Agent orchestration with multi-provider LLM support

use std::sync::Arc;

use colony_channels::LlmMessage;
use colony_reflection::ReflectionObserver;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::{
    approval::{ApprovalConfig, ApprovalManager, ApprovalResponse},
    cancellation::{cancellable_stream, CancellationToken},
    context::{ContextConfig, ContextManager},
    persistence::ConversationStore,
    provider::{
        CompletionRequest, Message, MessageContent, MessageRole, ModelProvider, ToolDefinition,
    },
    router::ModelRouter,
    token_accounting::{BudgetWarning, RequestUsage, TokenBudget, TokenTracker},
    tool::{ToolOutput, ToolRegistry},
    AgentError, ProviderType, Result, StreamEvent,
};

/// Colony Shell agent with multi-provider support
pub struct Agent {
    router: ModelRouter,
    messages: Vec<Message>,
    tools: ToolRegistry,
    config: AgentConfig,
    approval: ApprovalManager,
    token_tracker: TokenTracker,
    context_manager: ContextManager,
    conversation_store: Option<ConversationStore>,
    conversation_id: Option<String>,
    /// Reflection observer
    observer: Option<Arc<dyn ReflectionObserver>>,
    /// Channel writer for LLM conversation (fd 4 / stdllm)
    llm_channel_tx: Option<mpsc::UnboundedSender<LlmMessage>>,
    /// Tool calls the model has asked for since this agent was created, refused ones included.
    tool_calls: u32,
}

/// Agent configuration
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub max_tokens: u32,
    pub system_prompt: String,
    pub use_caching: bool,
    pub temperature: Option<f32>,
    pub approval_config: ApprovalConfig,
    pub token_budget: Option<TokenBudget>,
    pub context_config: ContextConfig,
    pub persistence_enabled: bool,
    pub memory_enabled: bool,
    pub auto_approve_patterns: Vec<String>,
    /// Most tool calls this agent may run. Calls past it are refused and the model is told so.
    pub max_tool_calls: Option<u32>,
    /// Print the model's text to stdout as it streams (`process_full`). Off for unattended runs.
    pub echo_stream: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            system_prompt: "You are Colony Shell's agent assistant.".to_string(),
            use_caching: true,
            temperature: None,
            approval_config: ApprovalConfig::default(),
            token_budget: None,
            context_config: ContextConfig::default(),
            persistence_enabled: false,
            memory_enabled: false,
            auto_approve_patterns: Vec::new(),
            max_tool_calls: None,
            echo_stream: true,
        }
    }
}

impl AgentConfig {
    /// Apply system_prompt, max_tokens, and temperature from an `AgentDefinition`.
    fn apply_definition(&mut self, definition: &colony_exec::special_files::AgentDefinition) {
        if !definition.prompt_template.is_empty() {
            self.system_prompt = definition.prompt_template.clone();
        } else if !definition.description.is_empty() {
            self.system_prompt = definition.description.clone();
        }

        if let Some(max_tokens) = definition.capabilities.max_tokens {
            self.max_tokens = max_tokens;
        }

        if let Some(temperature) = definition.capabilities.temperature {
            self.temperature = Some(temperature);
        }
    }
}

/// Emit a streaming text chunk to stdout, flushing immediately.
///
/// Prints OSC 2337 sequences for colony-terminal when connected.
fn emit_thought(text: &str) -> Result<()> {
    print!("{}", text);
    use std::io::Write;
    std::io::stdout()
        .flush()
        .map_err(|e| AgentError::Protocol(format!("failed to flush stdout: {}", e)))?;
    Ok(())
}

/// Log a tool call for debugging; emits OSC 2337 when colony-terminal is connected.
fn emit_tool_call(tool_name: &str, input: &serde_json::Value) -> Result<()> {
    debug!("Tool call: {} with input: {:?}", tool_name, input);
    Ok(())
}

impl Agent {
    /// Create new agent with auto-detected provider
    pub async fn new() -> Result<Self> {
        Self::with_config(AgentConfig::default()).await
    }

    /// Create an agent from an AGENT.md definition.
    pub async fn from_definition(
        definition: &colony_exec::special_files::AgentDefinition,
    ) -> Result<Self> {
        let mut config = AgentConfig::default();
        config.apply_definition(definition);

        if let Some(ref model_spec) = definition.capabilities.model {
            if let Some((provider_str, model_name)) = model_spec.split_once(':') {
                if let Some(provider_type) = ProviderType::parse(provider_str) {
                    return Self::with_provider(provider_type, model_name, config).await;
                }
            }

            let provider_type = infer_provider_from_model(model_spec);
            return Self::with_provider(provider_type, model_spec.as_str(), config).await;
        }

        Self::with_config(config).await
    }

    /// Reconfigure this agent from an AGENT.md definition.
    pub async fn configure_from_definition(
        &mut self,
        definition: &colony_exec::special_files::AgentDefinition,
    ) -> Result<()> {
        self.config.apply_definition(definition);

        if let Some(ref model_spec) = definition.capabilities.model {
            let (provider_type, model_name) =
                if let Some((provider_str, model_name)) = model_spec.split_once(':') {
                    let pt = ProviderType::parse(provider_str)
                        .unwrap_or_else(|| infer_provider_from_model(model_spec));
                    (pt, model_name.to_string())
                } else {
                    (infer_provider_from_model(model_spec), model_spec.clone())
                };

            let current = self.router.current_model();
            if current != model_name {
                self.switch_model(provider_type, model_name).await?;
            }
        }

        self.messages.clear();

        info!(
            "Agent reconfigured from definition '{}': model={}, max_tokens={}",
            definition.name,
            self.router.current_model(),
            self.config.max_tokens,
        );

        Ok(())
    }

    /// Create agent with custom configuration
    pub async fn with_config(config: AgentConfig) -> Result<Self> {
        let router = ModelRouter::auto_detect().await?;
        info!(
            "Agent initialized with provider: {}, model: {}",
            router.provider_name(),
            router.model_name()
        );
        Self::from_router(router, config)
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
        Self::from_router(router, config)
    }

    /// Shared constructor body: initialise an `Agent` from an already-configured router.
    fn from_router(router: ModelRouter, config: AgentConfig) -> Result<Self> {
        let mut tools = ToolRegistry::with_builtins();
        if !config.auto_approve_patterns.is_empty() {
            tools.set_auto_approve(config.auto_approve_patterns.clone());
        }

        let approval = ApprovalManager::new(config.approval_config.clone());

        let token_tracker = match &config.token_budget {
            Some(budget) => TokenTracker::new().with_budget(budget.clone()),
            None => TokenTracker::new(),
        };

        let context_manager = ContextManager::new(config.context_config.clone());

        Ok(Self {
            router,
            messages: Vec::new(),
            tools,
            config,
            approval,
            token_tracker,
            context_manager,
            conversation_store: None,
            conversation_id: None,
            observer: None,
            llm_channel_tx: None,
            tool_calls: 0,
        })
    }

    /// Enable conversation persistence with a SQLite store
    pub fn enable_persistence(
        &mut self,
        store: ConversationStore,
        conversation_id: impl Into<String>,
    ) {
        let id = conversation_id.into();
        self.token_tracker.set_conversation(id.clone());
        self.conversation_id = Some(id);
        self.conversation_store = Some(store);
    }

    /// Resume an existing conversation from persistence
    pub fn resume_conversation(&mut self, conversation_id: &str) -> Result<()> {
        let Some(ref store) = self.conversation_store else {
            return Err(AgentError::Conversation(
                "persistence not enabled".to_string(),
            ));
        };

        let messages = store.resume_conversation(conversation_id)?;
        self.messages = messages;
        self.conversation_id = Some(conversation_id.to_string());
        self.token_tracker.set_conversation(conversation_id);

        info!(
            "Resumed conversation {} with {} messages",
            conversation_id,
            self.messages.len()
        );

        Ok(())
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

    /// Switch to a model from a "provider:model" spec string.
    pub async fn switch_to(&mut self, spec: &str) -> Result<()> {
        let (provider_type, model_name) = if let Some((provider_str, model_name)) =
            spec.split_once(':')
        {
            let pt = ProviderType::parse(provider_str)
                .ok_or_else(|| AgentError::Protocol(format!("unknown provider: {provider_str}")))?;
            (pt, model_name.to_string())
        } else {
            (infer_provider_from_model(spec), spec.to_string())
        };
        self.switch_model(provider_type, model_name).await
    }

    /// Get current provider info
    pub fn current_provider(&self) -> (&ProviderType, &str) {
        (self.router.current_provider(), self.router.current_model())
    }

    /// Get mutable access to tool registry
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.tools
    }

    /// Get token usage stats
    pub fn usage_stats(&self) -> &crate::token_accounting::UsageStats {
        self.token_tracker.stats()
    }

    /// Get budget warning level
    pub fn budget_warning(&self) -> BudgetWarning {
        self.token_tracker.budget_warning()
    }

    /// Get context utilization
    pub fn context_utilization(&self) -> f64 {
        self.context_manager.utilization()
    }

    /// Set a reflection observer for recording agent chat observations.
    pub fn set_observer(&mut self, observer: Arc<dyn ReflectionObserver>) {
        self.observer = Some(observer);
    }

    /// Set the LLM channel writer for forwarding conversation to fd 4 (stdllm).
    ///
    /// Returns the receiving end so the caller can drain messages into a `ChannelManager`.
    pub fn set_llm_channel(&mut self) -> mpsc::UnboundedReceiver<LlmMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.llm_channel_tx = Some(tx);
        rx
    }

    /// Send a message to the LLM channel if a writer is configured.
    fn emit_to_llm_channel(&self, msg: LlmMessage) {
        if let Some(ref tx) = self.llm_channel_tx {
            let _ = tx.send(msg);
        }
    }

    /// Process user input and return the model's text response.
    ///
    /// **Text-only**: this method collects text deltas but silently discards any
    /// tool-call events the model may emit. If you need tool execution, use
    /// [`process_full`](Self::process_full) instead.
    pub async fn process(&mut self, user_input: impl Into<String>) -> Result<String> {
        let cancel = CancellationToken::new();
        self.process_with_cancellation(user_input, &cancel).await
    }

    /// Process user input with cancellation support (text-only, no tool
    /// execution — see [`process`](Self::process)).
    pub async fn process_with_cancellation(
        &mut self,
        user_input: impl Into<String>,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let input = user_input.into();
        info!("Processing user input: {}", input);

        // Emit user message to LLM channel (fd 4)
        self.emit_to_llm_channel(LlmMessage::new(colony_channels::LlmRole::User, &input));

        let user_msg = Message {
            role: MessageRole::User,
            content: MessageContent::Text(input),
        };

        // Persist user message
        if let (Some(store), Some(conv_id)) = (&self.conversation_store, &self.conversation_id) {
            store.save_message(conv_id, &user_msg, None)?;
        }

        self.messages.push(user_msg);

        if self.context_manager.should_prune(&self.messages) {
            self.messages = self.context_manager.prune(&self.messages);
            info!(
                "Context pruned to {} messages ({} estimated tokens)",
                self.messages.len(),
                self.context_manager.estimated_tokens()
            );
        }

        let tool_defs = self.convert_tool_definitions();

        let mut request = CompletionRequest::new()
            .with_system(&self.config.system_prompt)
            .with_max_tokens(self.config.max_tokens)
            .with_tools(tool_defs);

        if self.config.use_caching {
            request = request.enable_caching();
        }

        request.messages = self.messages.clone();

        let stream = self.router.complete(request).await?;
        let result = cancellable_stream(stream, cancel).await?;

        if result.was_cancelled {
            warn!(
                "Stream cancelled by user after {} events",
                result.events_received
            );
        }

        let response_text = result.text;

        let output_tokens_est = self.record_usage_estimate(&response_text);
        match self.token_tracker.budget_warning() {
            BudgetWarning::Warning => {
                warn!(
                    "Token budget warning: {} remaining",
                    self.token_tracker.budget_remaining().unwrap_or(0)
                );
            }
            BudgetWarning::Critical => {
                warn!("Token budget CRITICAL - nearing limit");
            }
            BudgetWarning::Exceeded => {
                warn!("Token budget EXCEEDED");
            }
            BudgetWarning::None => {}
        }

        if !response_text.is_empty() {
            // Emit assistant response to LLM channel (fd 4)
            self.emit_to_llm_channel(LlmMessage::new(
                colony_channels::LlmRole::Assistant,
                &response_text,
            ));

            let assistant_msg = Message {
                role: MessageRole::Assistant,
                content: MessageContent::Text(response_text.clone()),
            };

            if let (Some(store), Some(conv_id)) = (&self.conversation_store, &self.conversation_id)
            {
                store.save_message(conv_id, &assistant_msg, Some(output_tokens_est as i64))?;
            }

            self.messages.push(assistant_msg);
        }

        self.context_manager.update_estimate(&self.messages);

        Ok(response_text)
    }

    /// Process user input with full streaming event handling, including tool
    /// execution.
    ///
    /// Unlike [`process`](Self::process), this method intercepts `ToolCallComplete`
    /// events, runs the requested tool through the approval pipeline, and feeds
    /// the result back into the conversation.
    pub async fn process_full(&mut self, user_input: impl Into<String>) -> Result<String> {
        let input = user_input.into();
        info!("Processing user input (full): {}", input);

        // Emit user message to LLM channel (fd 4)
        self.emit_to_llm_channel(LlmMessage::new(colony_channels::LlmRole::User, &input));

        self.messages.push(Message {
            role: MessageRole::User,
            content: MessageContent::Text(input),
        });

        if self.context_manager.should_prune(&self.messages) {
            self.messages = self.context_manager.prune(&self.messages);
        }

        let tool_defs = self.convert_tool_definitions();

        let mut request = CompletionRequest::new()
            .with_system(&self.config.system_prompt)
            .with_max_tokens(self.config.max_tokens)
            .with_tools(tool_defs);

        if self.config.use_caching {
            request = request.enable_caching();
        }

        request.messages = self.messages.clone();

        let mut stream = self.router.complete(request).await?;
        let mut response_text = String::new();

        while let Some(event) = stream.next().await {
            match event? {
                StreamEvent::TextDelta { text, .. } => {
                    response_text.push_str(&text);
                    if self.config.echo_stream {
                        emit_thought(&text)?;
                    }
                }

                StreamEvent::ToolCallStart { id, name } => {
                    debug!("Tool call started: {} (id: {})", name, id);
                }

                StreamEvent::ToolCallComplete { id, name, input } => {
                    info!("Tool call complete: {} (id: {})", name, id);
                    self.handle_tool_use(id, name, input).await?;
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

        if !response_text.is_empty() {
            // Emit assistant response to LLM channel (fd 4)
            self.emit_to_llm_channel(LlmMessage::new(
                colony_channels::LlmRole::Assistant,
                &response_text,
            ));

            self.messages.push(Message {
                role: MessageRole::Assistant,
                content: MessageContent::Text(response_text.clone()),
            });
        }

        self.record_usage_estimate(&response_text);
        self.context_manager.update_estimate(&self.messages);

        // Record agent chat observation
        if let Some(ref observer) = self.observer {
            let (provider_type, _) = self.current_provider();
            observer
                .observe_agent_chat(colony_reflection::AgentChatData {
                    provider: format!("{:?}", provider_type).to_lowercase(),
                    model: self.router.current_model().to_string(),
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    duration_ms: 0,
                })
                .await;
        }

        Ok(response_text)
    }

    /// Record a request's token usage, estimated at four characters a token (the providers'
    /// streams carry no usage counts). Returns the output estimate.
    // ponytail: character estimate, replace with provider usage counts once streams report them
    fn record_usage_estimate(&mut self, response_text: &str) -> u64 {
        let input_tokens = self
            .messages
            .iter()
            .map(|m| match &m.content {
                MessageContent::Text(t) => t.len() as u64 / 4,
                MessageContent::Blocks(_) => 50,
            })
            .sum::<u64>();
        let output_tokens = response_text.len() as u64 / 4;
        self.token_tracker.record(RequestUsage {
            input_tokens,
            output_tokens,
        });
        output_tokens
    }

    /// Tool calls the model has asked for so far, refused ones included.
    pub fn tool_call_count(&self) -> u32 {
        self.tool_calls
    }

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

    async fn handle_tool_use(
        &mut self,
        tool_use_id: String,
        tool_name: String,
        input: serde_json::Value,
    ) -> Result<()> {
        info!("Tool use: {} (id: {})", tool_name, tool_use_id);

        self.tool_calls += 1;
        if let Some(max) = self
            .config
            .max_tool_calls
            .filter(|max| self.tool_calls > *max)
        {
            warn!("Tool call {} refused: limit of {} reached", tool_name, max);
            self.messages.push(Message {
                role: MessageRole::User,
                content: MessageContent::Text(format!(
                    "Tool result: refused, the limit of {max} tool calls is reached. \
                     Answer with what you have."
                )),
            });
            return Ok(());
        }

        let classification = self
            .tools
            .classification(&tool_name)
            .ok_or_else(|| AgentError::ToolExecution(format!("unknown tool: {}", tool_name)))?;

        let result = if self.approval.needs_approval(&tool_name, classification)
            && !self.tools.is_auto_approved(&tool_name)
        {
            let approval = self.approval.request_approval(&tool_name, &input).await?;

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
        } else {
            emit_tool_call(&tool_name, &input)?;
            self.execute_tool(&tool_name, input).await?
        };

        let result_str = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());

        // Emit tool result to LLM channel (fd 4) for conversation logging
        self.emit_to_llm_channel(
            LlmMessage::new(
                colony_channels::LlmRole::Tool,
                format!("Tool result ({}): {}", tool_name, result_str),
            )
            .with_name(&tool_name)
            .with_tool_call_id(&tool_use_id),
        );

        self.messages.push(Message {
            role: MessageRole::User,
            content: MessageContent::Text(format!("Tool result: {}", result_str)),
        });

        Ok(())
    }

    async fn execute_tool(&self, tool_name: &str, input: serde_json::Value) -> Result<ToolOutput> {
        self.tools
            .execute(tool_name, input)
            .await
            .map_err(|e| AgentError::ToolExecution(e.to_string()))
    }

    pub fn system_prompt(&self) -> &str {
        &self.config.system_prompt
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Get the conversation ID if persistence is active
    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    /// Get message count in current conversation
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get tool registry (read-only)
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Get conversation store (if persistence is enabled)
    pub fn conversation_store(&self) -> Option<&ConversationStore> {
        self.conversation_store.as_ref()
    }

    /// Clear conversation history, keeping system prompt
    pub fn clear_messages(&mut self) {
        self.messages
            .retain(|m| matches!(m.role, MessageRole::System));
        self.context_manager.update_estimate(&self.messages);
    }
}

/// Infer provider type from model name string.
fn infer_provider_from_model(model_name: &str) -> ProviderType {
    let lower = model_name.to_lowercase();
    if lower.starts_with("claude")
        || lower.contains("sonnet")
        || lower.contains("opus")
        || lower.contains("haiku")
    {
        ProviderType::Anthropic
    } else if lower.starts_with("gpt")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("chatgpt")
    {
        ProviderType::OpenAI
    } else {
        ProviderType::Ollama
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A provider whose every turn asks for `file_read` of one file `calls` times, then says `text`.
    struct Scripted {
        path: String,
        calls: usize,
        text: &'static str,
    }

    #[async_trait::async_trait]
    impl ModelProvider for Scripted {
        async fn complete(
            &mut self,
            _request: CompletionRequest,
        ) -> Result<crate::provider::CompletionStream> {
            let mut events: Vec<Result<StreamEvent>> = (0..self.calls)
                .map(|i| {
                    Ok(StreamEvent::ToolCallComplete {
                        id: format!("t{i}"),
                        name: "file_read".to_string(),
                        input: serde_json::json!({ "path": self.path }),
                    })
                })
                .collect();
            events.push(Ok(StreamEvent::TextDelta {
                text: self.text.to_string(),
                index: 0,
            }));
            events.push(Ok(StreamEvent::Done));
            Ok(Box::pin(futures::stream::iter(events)))
        }
        fn max_context_tokens(&self) -> usize {
            100_000
        }
        fn provider_name(&self) -> &str {
            "scripted"
        }
        fn model_name(&self) -> &str {
            "scripted"
        }
        async fn health_check(&self) -> Result<crate::provider::ProviderHealth> {
            unimplemented!("not used")
        }
    }

    fn scripted_agent(
        calls: usize,
        text: &'static str,
        config: AgentConfig,
    ) -> (Agent, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, "secret-free note").unwrap();
        let provider = Scripted {
            path: path.display().to_string(),
            calls,
            text,
        };
        let agent =
            Agent::from_router(ModelRouter::with_provider(Box::new(provider)), config).unwrap();
        (agent, dir)
    }

    fn tool_results(agent: &Agent) -> Vec<String> {
        agent
            .messages
            .iter()
            .filter_map(|m| match &m.content {
                MessageContent::Text(t) if t.starts_with("Tool result:") => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn tool_calls_past_the_limit_are_refused_not_run() {
        let config = AgentConfig {
            max_tool_calls: Some(2),
            echo_stream: false,
            ..AgentConfig::default()
        };
        let (mut agent, _dir) = scripted_agent(3, "PASS: done", config);
        let text = agent.process_full("go").await.unwrap();

        assert_eq!(text, "PASS: done");
        assert_eq!(agent.tool_call_count(), 3);
        let results = tool_results(&agent);
        assert_eq!(results.len(), 3);
        assert!(results[0].contains("secret-free note") && results[1].contains("secret-free note"));
        assert!(
            results[2].contains("limit of 2 tool calls"),
            "{}",
            results[2]
        );
    }

    #[tokio::test]
    async fn process_full_counts_against_the_token_budget() {
        let config = AgentConfig {
            token_budget: Some(TokenBudget::new(10)),
            echo_stream: false,
            ..AgentConfig::default()
        };
        let (mut agent, _dir) = scripted_agent(
            0,
            "a reply long enough to pass ten tokens of budget",
            config,
        );
        assert_eq!(agent.budget_warning(), BudgetWarning::None);
        agent.process_full("go").await.unwrap();
        assert_eq!(agent.budget_warning(), BudgetWarning::Exceeded);
    }

    #[test]
    fn test_infer_provider_claude_models() {
        assert_eq!(
            infer_provider_from_model("claude-3-5-sonnet-20241022"),
            ProviderType::Anthropic
        );
        assert_eq!(
            infer_provider_from_model("claude-sonnet-4-20250514"),
            ProviderType::Anthropic
        );
        assert_eq!(
            infer_provider_from_model("claude-3-opus"),
            ProviderType::Anthropic
        );
        assert_eq!(
            infer_provider_from_model("claude-3-haiku"),
            ProviderType::Anthropic
        );
    }

    #[test]
    fn test_infer_provider_openai_models() {
        assert_eq!(
            infer_provider_from_model("gpt-4-turbo"),
            ProviderType::OpenAI
        );
        assert_eq!(infer_provider_from_model("gpt-4o"), ProviderType::OpenAI);
        assert_eq!(
            infer_provider_from_model("o1-preview"),
            ProviderType::OpenAI
        );
        assert_eq!(infer_provider_from_model("o3-mini"), ProviderType::OpenAI);
    }

    #[test]
    fn test_infer_provider_ollama_models() {
        assert_eq!(infer_provider_from_model("llama3"), ProviderType::Ollama);
        assert_eq!(infer_provider_from_model("mistral"), ProviderType::Ollama);
        assert_eq!(infer_provider_from_model("codellama"), ProviderType::Ollama);
    }

    #[test]
    fn test_agent_config_defaults() {
        let config = AgentConfig::default();
        assert_eq!(config.max_tokens, 4096);
        assert!(config.use_caching);
        assert!(config.temperature.is_none());
        assert_eq!(
            config.system_prompt,
            "You are Colony Shell's agent assistant."
        );
        assert!(!config.persistence_enabled);
        assert!(!config.memory_enabled);
        assert!(config.auto_approve_patterns.is_empty());
    }

    #[test]
    fn test_agent_config_with_budget() {
        let config = AgentConfig {
            token_budget: Some(TokenBudget::new(500_000)),
            ..Default::default()
        };
        assert!(config.token_budget.is_some());
    }

    #[test]
    fn test_agent_config_with_auto_approve() {
        let config = AgentConfig {
            auto_approve_patterns: vec!["file_read".to_string(), "search".to_string()],
            ..Default::default()
        };
        assert_eq!(config.auto_approve_patterns.len(), 2);
    }
}
