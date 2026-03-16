//! Recommendation source trait and core types.

use serde::{Deserialize, Serialize};

/// Where a recommendation came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecommendKind {
    History,
    Command,
    Path,
    File,
    Directory,
    Variable,
    Git,
    Builtin,
    Ai,
}

impl RecommendKind {
    /// Short badge string for display.
    pub fn badge(&self) -> &'static str {
        match self {
            Self::History => "HIST",
            Self::Command => "CMD",
            Self::Path => "PATH",
            Self::File => "FILE",
            Self::Directory => "DIR",
            Self::Variable => "VAR",
            Self::Git => "GIT",
            Self::Builtin => "BLT",
            Self::Ai => "AI",
        }
    }
}

/// A single recommendation.
#[derive(Debug, Clone)]
pub struct Recommendation {
    /// Text to insert (replaces current word or appends).
    pub text: String,
    /// Display label.
    pub label: String,
    /// Optional description.
    pub description: Option<String>,
    /// Source kind.
    pub kind: RecommendKind,
    /// Confidence score 0.0--1.0.
    pub confidence: f64,
    /// Character indices in label that matched (for highlighting).
    pub match_indices: Vec<usize>,
}

/// Trait for recommendation sources.
pub trait RecommendSource: Send + Sync {
    /// Generate recommendations for the given input context.
    fn recommend(&self, input: &super::input::RecommendInput) -> Vec<Recommendation>;

    /// Source identifier for debugging.
    fn name(&self) -> &'static str;
}
