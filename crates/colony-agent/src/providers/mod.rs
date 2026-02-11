//! Model provider implementations
//!
//! Concrete implementations of the ModelProvider trait for different LLM services:
//! - Anthropic: Claude models via our Rust SDK
//! - OpenAI: GPT models via ChatCompletions API
//! - Ollama: Local models (free fallback)
//! - Colony Cloud: Managed LLM service (colony-cloud-sdk)

pub mod anthropic;
#[cfg(feature = "cloud")]
pub mod cloud;
pub mod ollama;
pub mod openai;

pub use anthropic::AnthropicProvider;
#[cfg(feature = "cloud")]
pub use cloud::CloudProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
