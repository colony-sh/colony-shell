//! Buffer management system (F025)
//!
//! Styled text buffers with ANSI parsing, scrollback, and search support.
//!
//! - `StyledBuffer` — line-based buffer with ANSI style spans
//! - `StyledSpan` — text range with foreground/background color, bold/italic/underline
//! - `BufferView` — scrollable window into a buffer
//! - ANSI escape parser — converts raw terminal output into `StyledBuffer`
//! - Search — find matches in buffer content

mod ansi;
mod buffer;
mod search;
mod span;
mod view;

pub use ansi::{parse_ansi, parse_ansi_line};
pub use buffer::StyledBuffer;
pub use search::{search_buffer, search_buffer_case_insensitive, SearchResult};
pub use span::{Color, StyledSpan};
pub use view::BufferView;
