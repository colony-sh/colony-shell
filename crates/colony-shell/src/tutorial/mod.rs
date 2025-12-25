//! Tutorial mode: Script X-Ray
//!
//! Provides X-ray vision into any shell script's execution.

pub mod complexity;
pub mod events;
pub mod runner;
pub mod teach;
pub mod tracer;

pub mod explain;
pub mod tui;

pub use complexity::ComplexityScorer;
pub use events::{ComplexityLevel, ExpansionPhase, TraceEvent};
pub use runner::{TutorialConfig, TutorialRunner};
pub use teach::{Concept, ConceptTracker};
pub use tracer::{TraceDepth, TraceEmitter, TracerConfig};
pub use tui::TutorialTui;
