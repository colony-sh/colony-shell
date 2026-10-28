//! Memory substrate with LanceDB vectors and SQLite metadata
//!
//! This crate provides the semantic memory layer for Colony Shell's AI-native features.
//! It uses LanceDB for vector storage and fastembed for generating embeddings.
//!
//! ## Architecture
//!
//! - **LanceDB**: Stores vector embeddings for semantic search (384d via all-MiniLM-L6-v2)
//! - **Scopes**: Hierarchical memory organization (Global → Project → Session → Task)
//! - **Operations**: Store, retrieve, search, consolidate, forget
//!
//! ## Example
//!
//! ```rust,no_run
//! use colony_memory::{MemoryStore, MemoryEntry, Scope};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let store = MemoryStore::open_default().await?;
//!
//! // Store a memory
//! store.store(
//!     "The user prefers verbose error messages",
//!     Scope::Project("my-project".to_string()),
//!     None,
//! ).await?;
//!
//! // Search semantically
//! let results = store.search("error handling preferences", 5).await?;
//! for result in results {
//!     println!("{}: {}", result.entry.id, result.entry.content);
//! }
//! # Ok(())
//! # }
//! ```

mod embedding;
mod error;
mod scope;
mod store;

pub mod context;
pub mod decay;
pub mod eviction;
pub mod query;
pub mod temperature;

pub use context::{ConversationRole, ConversationTurn, MemoryBackedContext};
pub use decay::{DecayConfig, ImportanceDecayScheduler};
pub use embedding::{EmbeddingProvider, FastEmbedProvider};
pub use error::{MemoryError, Result};
pub use eviction::{CompositeEvictionPolicy, EvictionPolicy, LruEvictionPolicy, TtlEvictionPolicy};
pub use query::{MetadataCondition, MetadataOperator, MetadataQuery};
pub use scope::Scope;
pub use store::{MemoryEntry, MemoryStore, SearchResult};
pub use temperature::{MemoryTemperature, TemperatureConfig};
