//! Pane management and layout system (F026)
//!
//! Shell-side pane semantics that translate to terminal pane operations
//! via OSC 2337 escape sequences. Provides builtin commands for splitting,
//! focusing, closing, and applying layout presets.
//!
//! ## Builtin Commands
//!
//! - `split [--horizontal|--vertical]` — split current pane
//! - `vsplit` — shorthand for vertical split
//! - `layout <name>` — apply named layout preset
//! - `pane focus <direction>` — move focus (left/right/up/down)
//! - `pane close` — close current pane

mod builtins;
mod layout;

pub use builtins::{builtin_layout, builtin_pane, builtin_split, builtin_vsplit};
pub use layout::{LayoutPreset, PaneDefinition, PaneLayout};
