//! Programmable prompt component system for Colony Shell
//!
//! Provides a component-based prompt where users compose prompts from
//! built-in and custom components that react to tuple space events.

pub mod component;
pub mod context;

pub use component::{Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern};
pub use context::ComponentContext;
