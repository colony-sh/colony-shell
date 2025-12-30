//! Programmable prompt component system for Colony Shell
//!
//! Provides a component-based prompt where users compose prompts from
//! built-in and custom components that react to tuple space events.

pub mod builtins;
pub mod component;
pub mod context;
pub mod manager;
pub mod registry;
pub mod render;
pub mod subscription;

pub use builtins::{
    ExitStatusComponent, GitBranchComponent, GitStatusComponent, JobsComponent, ModelComponent,
    PwdComponent,
};
pub use component::{Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern};
pub use context::ComponentContext;
pub use manager::PromptManager;
pub use registry::ComponentRegistry;
pub use render::ansi::{render_outputs, render_prompt, strip_ansi};
pub use subscription::SubscriptionManager;
