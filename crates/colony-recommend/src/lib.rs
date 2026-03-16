//! Colony Shell command recommendation engine.
//!
//! Provides contextual command suggestions from multiple sources:
//! history, PATH commands, file paths, git refs, and (optionally) AI.

pub mod engine;
pub mod input;
pub mod scoring;
pub mod source;
pub mod sources;

pub use engine::RecommendationEngine;
pub use input::RecommendInput;
pub use source::{RecommendKind, RecommendSource, Recommendation};
