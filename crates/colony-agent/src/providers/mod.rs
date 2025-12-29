//! Model provider implementations
//!
//! Concrete implementations of the ModelProvider trait for different LLM services:
//! - Anthropic: Claude models via our Rust SDK
//! - OpenAI: GPT models via ChatCompletions API
//! - Ollama: Local models (free fallback)

pub mod anthropic;
pub mod ollama;
pub mod openai;

pub use anthropic::AnthropicProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
