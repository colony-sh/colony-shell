//! Colony Shell debugger and X-Ray tracer.
//!
//! Provides `DebugController` for breakpoints and stepping,
//! `TraceEvent` for structured execution tracing, and a
//! split-pane ratatui TUI for both debug and X-Ray modes.

pub mod breakpoint;
pub mod complexity;
pub mod controller;
pub mod events;
pub mod explain;
pub mod tui;

pub use events::{Complexity, TraceEvent};
