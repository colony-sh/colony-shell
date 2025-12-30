//! Programmable prompt component system for Colony Shell
//!
//! Provides a component-based prompt where users compose prompts from
//! built-in and custom components that react to tuple space events.

pub mod builtins;
pub mod component;
pub mod context;
pub mod render;

pub use builtins::PwdComponent;
pub use component::{Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern};
pub use context::ComponentContext;
pub use render::ansi::{render_outputs, render_prompt, strip_ansi};
