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
pub mod widgets;

pub use builtins::{
    ExitStatusComponent, FileTreeSidebarComponent, GitBranchComponent, GitFilesSidebarComponent,
    GitStatusComponent, JobsComponent, LspComponent, McpSidebarComponent, ModelComponent,
    PwdComponent, SessionSidebarComponent,
};
pub use component::{
    Color, ComponentOutput, InteractiveConfig, PromptComponent, SidebarComponent, SidebarLine,
    SidebarOutput, SidebarPosition, Style, StyledSpan, TuplePattern,
};
pub use context::ComponentContext;
pub use manager::{color_to_string, ComponentData, PromptData, PromptManager};
pub use registry::ComponentRegistry;
pub use render::ansi::{render_outputs, render_prompt, strip_ansi};
pub use subscription::SubscriptionManager;
pub use widgets::{
    checkout_remote, create_branch, gather_branch_data, switch_branch, BranchPickerData,
    LocalBranch, RemoteBranch,
};
