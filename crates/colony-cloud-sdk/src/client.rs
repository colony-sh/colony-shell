//! Colony Cloud API client
//!
//! Typed HTTP client for the Colony Cloud REST API.
//! Uses reqwest with async/streaming support.

use crate::config::CloudConfig;
use crate::error::{CloudError, Result};
use crate::models::{
    ApiErrorResponse, ChatRequest, ChatResponse, ChatStreamChunk, Model, ModelsResponse,
    RecipeResponse, RecipeSubmission,
};
use futures::stream::{self, Stream};
use reqwest::Client;
use std::pin::Pin;
use tracing::{debug, warn};

/// Typed client for Colony Cloud API
pub struct ColonyClient {
    http: Client,
    config: CloudConfig,
}

impl ColonyClient {
    /// Create client from environment (COLONY_API_KEY or ~/.colony/config.toml)
    pub fn from_env() -> Result<Self> {
        let config = CloudConfig::from_env()?;
        Ok(Self::from_config(config))
    }

    /// Create client with explicit API key
    pub fn new(api_key: impl Into<String>) -> Self {
        let config = CloudConfig::with_key(api_key);
        Self::from_config(config)
    }

    /// Create client from resolved config
    fn from_config(config: CloudConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    /// Override the base URL (for self-hosted instances or testing)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = url.into();
        self
    }

    /// Get the current base URL
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// List available models
    ///
    /// GET /v1/models
    pub async fn models(&self) -> Result<Vec<Model>> {
        let url = format!("{}/v1/models", self.config.base_url);
        debug!("Fetching models from {}", url);

        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(self.parse_error(status.as_u16(), response).await);
        }

        let models_response: ModelsResponse = response.json().await?;
        Ok(models_response.data)
    }

    /// Send a chat completion request (non-streaming)
    ///
    /// POST /v1/chat/completions
    pub async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/v1/chat/completions", self.config.base_url);
        debug!("Sending chat completion to {}", url);

        let mut req = request.clone();
        req.stream = Some(false);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&req)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(self.parse_error(status.as_u16(), response).await);
        }

        let chat_response: ChatResponse = response.json().await?;
        Ok(chat_response)
    }

    /// Send a streaming chat completion request
    ///
    /// POST /v1/chat/completions with stream=true
    /// Returns a stream of SSE chunks.
    pub async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatStreamChunk>> + Send>>> {
        let url = format!("{}/v1/chat/completions", self.config.base_url);
        debug!("Sending streaming chat completion to {}", url);

        let mut req = request.clone();
        req.stream = Some(true);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Accept", "text/event-stream")
            .json(&req)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(self.parse_error(status.as_u16(), response).await);
        }

        let sse_stream = parse_sse_stream(response);

        Ok(Box::pin(sse_stream))
    }

    /// Submit a recipe to the registry
    ///
    /// POST /v1/recipes
    pub async fn submit_recipe(&self, recipe: &RecipeSubmission) -> Result<RecipeResponse> {
        let url = format!("{}/v1/recipes", self.config.base_url);
        debug!("Submitting recipe '{}' to {}", recipe.name, url);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(recipe)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(self.parse_error(status.as_u16(), response).await);
        }

        let recipe_response: RecipeResponse = response.json().await?;
        Ok(recipe_response)
    }

    /// Check if the client can reach the API
    pub async fn health(&self) -> Result<bool> {
        let url = format!("{}/v1/models", self.config.base_url);
        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    async fn parse_error(&self, status: u16, response: reqwest::Response) -> CloudError {
        match response.json::<ApiErrorResponse>().await {
            Ok(err_body) => {
                if status == 401 || status == 403 {
                    CloudError::Auth(err_body.error.message)
                } else {
                    CloudError::Api {
                        status,
                        message: err_body.error.message,
                    }
                }
            }
            Err(_) => CloudError::Api {
                status,
                message: format!("HTTP {}", status),
            },
        }
    }
}

/// Parse a reqwest response as an SSE stream into typed chunks.
///
/// SSE format:
/// ```text
/// data: {"id":"...","choices":[...]}
/// data: {"id":"...","choices":[...]}
/// data: [DONE]
/// ```
fn parse_sse_stream(
    response: reqwest::Response,
) -> impl Stream<Item = Result<ChatStreamChunk>> + Send {
    use futures::StreamExt;

    let byte_stream = response.bytes_stream();

    struct SseState<S> {
        inner: S,
        buffer: String,
    }

    stream::unfold(
        SseState {
            inner: Box::pin(byte_stream),
            buffer: String::new(),
        },
        |mut state| async move {
            loop {
                // Try to extract a complete SSE event from buffer
                while let Some(line_end) = state.buffer.find('\n') {
                    let line = state.buffer[..line_end]
                        .trim_end_matches('\r')
                        .to_string();
                    state.buffer = state.buffer[line_end + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };

                    if data == "[DONE]" {
                        return None;
                    }

                    match serde_json::from_str::<ChatStreamChunk>(data) {
                        Ok(chunk) => return Some((Ok(chunk), state)),
                        Err(e) => {
                            warn!("Failed to parse SSE chunk: {}", e);
                            continue;
                        }
                    }
                }

                // Need more data from the byte stream
                match state.inner.next().await {
                    Some(Ok(bytes)) => {
                        let text = match std::str::from_utf8(&bytes) {
                            Ok(t) => t.to_string(),
                            Err(e) => {
                                return Some((
                                    Err(CloudError::Stream(format!("invalid UTF-8: {}", e))),
                                    state,
                                ));
                            }
                        };
                        state.buffer.push_str(&text);
                    }
                    Some(Err(e)) => {
                        return Some((Err(CloudError::Stream(e.to_string())), state));
                    }
                    None => return None,
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_new() {
        let client = ColonyClient::new("test-key");
        assert_eq!(client.base_url(), "https://api.colony.sh");
    }

    #[test]
    fn test_client_with_base_url() {
        let client = ColonyClient::new("test-key").with_base_url("http://localhost:8080");
        assert_eq!(client.base_url(), "http://localhost:8080");
    }
}
