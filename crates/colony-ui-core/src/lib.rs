//! Programmable prompt component system for Colony Shell
//!
//! Provides a component-based prompt where users compose prompts from
//! built-in and custom components that react to tuple space events.

pub mod component;

pub use component::{ComponentOutput, PromptComponent, Style, StyledSpan};
