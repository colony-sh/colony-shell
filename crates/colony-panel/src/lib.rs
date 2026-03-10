//! Panel-as-process protocol and host for colony shell.
//!
//! Panels are child processes that render content via a structured JSON
//! protocol on stdout and receive input events on stdin. This crate provides
//! the protocol types, frame parser, layout engine, and (in future tasks)
//! the process management and host coordination layers.

pub mod host;
pub mod layout;
pub mod protocol;
pub mod slot;

pub use layout::{LayoutEngine, PanelConstraints, PanelId, PanelPosition, ResolvedLayout};
pub use protocol::{
    FrameLine, FrameParser, InputEvent, PanelConfig, PanelMessage, ProtocolError, StyleSpec,
    TupleEmit,
};
pub use host::{Focus, PanelHost, PanelSpec};
pub use slot::PanelSlot;
