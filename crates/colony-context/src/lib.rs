//! Colony Shell context management system.
//!
//! Persistent context storage, composition, and builtin commands
//! for managing AI interaction context.

pub mod builtin;
pub mod composer;
pub mod entry;
pub mod project;
pub mod store;

pub use composer::{ComposedContext, ContextComposer, ContextSection};
pub use entry::{ContextEntry, ContextFile, ContextKind, EntryMetadata, Priority};
pub use project::ProjectContext;
pub use store::{ContextStore, StoreError};
