# Part 8: Buffer Management

## 8.1 Overview

Buffers store the content displayed in the terminal UI. Each channel has an associated buffer, and multiple buffers can be displayed simultaneously in panes.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Terminal Window                                │
├─────────────────────────────────────────────────────────────────────────┤
│ ┌─────────────────────────────────┐ ┌─────────────────────────────────┐ │
│ │         Main Buffer             │ │       Context Buffer            │ │
│ │  $ echo "hello"                 │ │  [Observation] Started task     │ │
│ │  hello                          │ │  [Decision] Using blue-green    │ │
│ │  $ ls -la                       │ │  [Action] Created branch        │ │
│ │  total 32                       │ │  [Error] Build failed           │ │
│ │  drwxr-xr-x  5 user ...         │ │                                 │ │
│ │  $ _                            │ │                                 │ │
│ └─────────────────────────────────┘ └─────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────────────────────┐ │
│ │                          LLM Buffer                                  │ │
│ │  [user] Please review this code                                     │ │
│ │  [assistant] I'll analyze the code for potential issues...          │ │
│ └─────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

## 8.2 Buffer Structure

```rust
// crates/colony-channels/src/buffer.rs

use std::collections::VecDeque;

/// A buffer holds lines of styled text
pub struct Buffer {
    /// Channel this buffer is associated with
    channel_id: ChannelId,
    /// Buffer name (for display)
    name: String,
    /// Lines of content
    lines: VecDeque<BufferLine>,
    /// Maximum number of lines (scrollback)
    max_lines: usize,
    /// Current scroll position (0 = bottom)
    scroll_offset: usize,
    /// Whether buffer has unseen content
    has_unseen: bool,
    /// Last modification time
    last_modified: DateTime<Utc>,
    /// Unique buffer ID
    id: String,
}

/// A single line in a buffer
#[derive(Clone, Debug)]
pub struct BufferLine {
    /// The text content
    pub text: String,
    /// Per-character styling (parallel array)
    pub styles: Vec<CharStyle>,
    /// Line-level style
    pub line_style: LineStyle,
    /// Timestamp when line was added
    pub timestamp: DateTime<Utc>,
    /// Source information
    pub source: Option<LineSource>,
}

/// Style for a single character
#[derive(Clone, Copy, Debug, Default)]
pub struct CharStyle {
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub dim: bool,
}

/// Style for an entire line
#[derive(Clone, Debug, Default)]
pub struct LineStyle {
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub prefix: Option<String>,
    pub indent: usize,
}

/// Color specification
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    // Basic colors
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    // Bright variants
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    // 256 color
    Indexed(u8),
    // True color
    Rgb(u8, u8, u8),
}

/// Source of a line (for navigation)
#[derive(Clone, Debug)]
pub struct LineSource {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub command: Option<String>,
}

impl Buffer {
    pub fn new(channel_id: ChannelId, name: &str) -> Self {
        Self {
            channel_id,
            name: name.to_string(),
            lines: VecDeque::new(),
            max_lines: 10000,
            scroll_offset: 0,
            has_unseen: false,
            last_modified: Utc::now(),
            id: generate_id("buf"),
        }
    }
    
    pub fn with_max_lines(mut self, max: usize) -> Self {
        self.max_lines = max;
        self
    }
    
    /// Append a plain text line
    pub fn append_line(&mut self, text: &str, style: LineStyle) {
        let line = BufferLine {
            text: text.to_string(),
            styles: vec![CharStyle::default(); text.len()],
            line_style: style,
            timestamp: Utc::now(),
            source: None,
        };
        
        self.push_line(line);
    }
    
    /// Append a styled line
    pub fn append_styled(&mut self, text: &str, styles: Vec<CharStyle>, line_style: LineStyle) {
        let line = BufferLine {
            text: text.to_string(),
            styles,
            line_style,
            timestamp: Utc::now(),
            source: None,
        };
        
        self.push_line(line);
    }
    
    /// Append line with ANSI escape codes
    pub fn append_ansi(&mut self, text: &str) {
        let (plain_text, styles) = parse_ansi(text);
        self.append_styled(&plain_text, styles, LineStyle::default());
    }
    
    fn push_line(&mut self, line: BufferLine) {
        self.lines.push_back(line);
        self.last_modified = Utc::now();
        self.has_unseen = true;
        
        // Enforce max lines
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }
    
    /// Get visible lines for rendering
    pub fn visible_lines(&self, viewport_height: usize) -> impl Iterator<Item = &BufferLine> {
        let total = self.lines.len();
        let start = if total > viewport_height + self.scroll_offset {
            total - viewport_height - self.scroll_offset
        } else {
            0
        };
        let end = (total - self.scroll_offset).min(start + viewport_height);
        
        self.lines.range(start..end)
    }
    
    /// Scroll up (toward older content)
    pub fn scroll_up(&mut self, lines: usize) {
        let max_scroll = self.lines.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
    }
    
    /// Scroll down (toward newer content)
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }
    
    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }
    
    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = self.lines.len().saturating_sub(1);
    }
    
    /// Search for text
    pub fn search(&self, pattern: &str) -> Vec<SearchMatch> {
        let mut matches = Vec::new();
        
        for (line_idx, line) in self.lines.iter().enumerate() {
            for (pos, _) in line.text.match_indices(pattern) {
                matches.push(SearchMatch {
                    line: line_idx,
                    start: pos,
                    end: pos + pattern.len(),
                });
            }
        }
        
        matches
    }
    
    /// Clear buffer
    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
        self.has_unseen = false;
    }
    
    /// Mark as seen
    pub fn mark_seen(&mut self) {
        self.has_unseen = false;
    }
    
    /// Get line count
    pub fn len(&self) -> usize {
        self.lines.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
    
    /// Export to plain text
    pub fn to_plain_text(&self) -> String {
        self.lines.iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
    
    /// Export with ANSI codes
    pub fn to_ansi(&self) -> String {
        self.lines.iter()
            .map(|l| render_line_ansi(l))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Debug)]
pub struct SearchMatch {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

/// Parse ANSI escape codes into text + styles
fn parse_ansi(input: &str) -> (String, Vec<CharStyle>) {
    let mut plain = String::new();
    let mut styles = Vec::new();
    let mut current_style = CharStyle::default();
    
    let mut chars = input.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Parse escape sequence
            if chars.peek() == Some(&'[') {
                chars.next();
                let mut params = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == ';' {
                        params.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                
                if let Some(cmd) = chars.next() {
                    if cmd == 'm' {
                        // SGR (Select Graphic Rendition)
                        current_style = apply_sgr(&params, current_style);
                    }
                }
            }
        } else {
            plain.push(c);
            styles.push(current_style);
        }
    }
    
    (plain, styles)
}

/// Apply SGR parameters to style
fn apply_sgr(params: &str, mut style: CharStyle) -> CharStyle {
    for param in params.split(';') {
        match param.parse::<u8>() {
            Ok(0) => style = CharStyle::default(),
            Ok(1) => style.bold = true,
            Ok(2) => style.dim = true,
            Ok(3) => style.italic = true,
            Ok(4) => style.underline = true,
            Ok(9) => style.strikethrough = true,
            Ok(22) => { style.bold = false; style.dim = false; }
            Ok(23) => style.italic = false,
            Ok(24) => style.underline = false,
            Ok(29) => style.strikethrough = false,
            Ok(30) => style.foreground = Some(Color::Black),
            Ok(31) => style.foreground = Some(Color::Red),
            Ok(32) => style.foreground = Some(Color::Green),
            Ok(33) => style.foreground = Some(Color::Yellow),
            Ok(34) => style.foreground = Some(Color::Blue),
            Ok(35) => style.foreground = Some(Color::Magenta),
            Ok(36) => style.foreground = Some(Color::Cyan),
            Ok(37) => style.foreground = Some(Color::White),
            Ok(39) => style.foreground = None,
            Ok(40) => style.background = Some(Color::Black),
            Ok(41) => style.background = Some(Color::Red),
            Ok(42) => style.background = Some(Color::Green),
            Ok(43) => style.background = Some(Color::Yellow),
            Ok(44) => style.background = Some(Color::Blue),
            Ok(45) => style.background = Some(Color::Magenta),
            Ok(46) => style.background = Some(Color::Cyan),
            Ok(47) => style.background = Some(Color::White),
            Ok(49) => style.background = None,
            Ok(90) => style.foreground = Some(Color::BrightBlack),
            Ok(91) => style.foreground = Some(Color::BrightRed),
            Ok(92) => style.foreground = Some(Color::BrightGreen),
            Ok(93) => style.foreground = Some(Color::BrightYellow),
            Ok(94) => style.foreground = Some(Color::BrightBlue),
            Ok(95) => style.foreground = Some(Color::BrightMagenta),
            Ok(96) => style.foreground = Some(Color::BrightCyan),
            Ok(97) => style.foreground = Some(Color::BrightWhite),
            _ => {}
        }
    }
    style
}

/// Render line back to ANSI
fn render_line_ansi(line: &BufferLine) -> String {
    // Simplified - full implementation would track style changes
    line.text.clone()
}
```

## 8.3 Pane Management

```rust
// crates/colony-ui/src/pane.rs

use std::collections::HashMap;

/// Manages the pane layout
pub struct PaneManager {
    /// Root layout node
    root: LayoutNode,
    /// Buffer assignments
    buffer_assignments: HashMap<PaneId, ChannelId>,
    /// Currently focused pane
    focused: Option<PaneId>,
    /// Available presets
    presets: HashMap<String, LayoutPreset>,
}

pub type PaneId = String;

/// Layout tree node
#[derive(Clone, Debug)]
pub enum LayoutNode {
    /// Single pane showing a buffer
    Pane(PaneInfo),
    /// Horizontal split (left | right)
    Horizontal {
        left: Box<LayoutNode>,
        right: Box<LayoutNode>,
        ratio: f32,  // 0.0-1.0, left's share
    },
    /// Vertical split (top / bottom)
    Vertical {
        top: Box<LayoutNode>,
        bottom: Box<LayoutNode>,
        ratio: f32,  // 0.0-1.0, top's share
    },
    /// Tabbed panes
    Tabbed {
        tabs: Vec<LayoutNode>,
        active: usize,
    },
}

#[derive(Clone, Debug)]
pub struct PaneInfo {
    pub id: PaneId,
    pub title: String,
    pub channel_id: Option<ChannelId>,
    pub visible: bool,
    pub scrollback: usize,
}

/// Predefined layout configuration
#[derive(Clone, Debug)]
pub struct LayoutPreset {
    pub name: String,
    pub description: String,
    pub layout: LayoutNode,
}

impl PaneManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            root: LayoutNode::Pane(PaneInfo {
                id: "main".to_string(),
                title: "Main".to_string(),
                channel_id: Some(STDOUT),
                visible: true,
                scrollback: 10000,
            }),
            buffer_assignments: HashMap::new(),
            focused: Some("main".to_string()),
            presets: HashMap::new(),
        };
        
        mgr.register_default_presets();
        mgr
    }
    
    fn register_default_presets(&mut self) {
        // Default: just main
        self.presets.insert("default".to_string(), LayoutPreset {
            name: "default".to_string(),
            description: "Single main pane".to_string(),
            layout: LayoutNode::Pane(PaneInfo {
                id: "main".to_string(),
                title: "Main".to_string(),
                channel_id: Some(STDOUT),
                visible: true,
                scrollback: 10000,
            }),
        });
        
        // Dev: main + context + llm
        self.presets.insert("dev".to_string(), LayoutPreset {
            name: "dev".to_string(),
            description: "Main with context and LLM sidebars".to_string(),
            layout: LayoutNode::Horizontal {
                left: Box::new(LayoutNode::Pane(PaneInfo {
                    id: "main".to_string(),
                    title: "Main".to_string(),
                    channel_id: Some(STDOUT),
                    visible: true,
                    scrollback: 10000,
                })),
                right: Box::new(LayoutNode::Vertical {
                    top: Box::new(LayoutNode::Pane(PaneInfo {
                        id: "context".to_string(),
                        title: "Context".to_string(),
                        channel_id: Some(STDCONTEXT),
                        visible: true,
                        scrollback: 1000,
                    })),
                    bottom: Box::new(LayoutNode::Pane(PaneInfo {
                        id: "llm".to_string(),
                        title: "LLM".to_string(),
                        channel_id: Some(STDLLM),
                        visible: true,
                        scrollback: 500,
                    })),
                    ratio: 0.5,
                }),
                ratio: 0.7,
            },
        });
        
        // Debug: main + context + tool + diagnostic
        self.presets.insert("debug".to_string(), LayoutPreset {
            name: "debug".to_string(),
            description: "Full debug layout with all channels".to_string(),
            layout: LayoutNode::Vertical {
                top: Box::new(LayoutNode::Horizontal {
                    left: Box::new(LayoutNode::Pane(PaneInfo {
                        id: "main".to_string(),
                        title: "Main".to_string(),
                        channel_id: Some(STDOUT),
                        visible: true,
                        scrollback: 10000,
                    })),
                    right: Box::new(LayoutNode::Pane(PaneInfo {
                        id: "context".to_string(),
                        title: "Context".to_string(),
                        channel_id: Some(STDCONTEXT),
                        visible: true,
                        scrollback: 1000,
                    })),
                    ratio: 0.6,
                }),
                bottom: Box::new(LayoutNode::Horizontal {
                    left: Box::new(LayoutNode::Pane(PaneInfo {
                        id: "tool".to_string(),
                        title: "Tools".to_string(),
                        channel_id: Some(STDTOOL),
                        visible: true,
                        scrollback: 200,
                    })),
                    right: Box::new(LayoutNode::Pane(PaneInfo {
                        id: "diagnostic".to_string(),
                        title: "Diagnostic".to_string(),
                        channel_id: Some(STDDIAG),
                        visible: true,
                        scrollback: 5000,
                    })),
                    ratio: 0.5,
                }),
                ratio: 0.7,
            },
        });
        
        // Agent: main + context + llm + tool (tabbed)
        self.presets.insert("agent".to_string(), LayoutPreset {
            name: "agent".to_string(),
            description: "Agent development layout".to_string(),
            layout: LayoutNode::Horizontal {
                left: Box::new(LayoutNode::Pane(PaneInfo {
                    id: "main".to_string(),
                    title: "Main".to_string(),
                    channel_id: Some(STDOUT),
                    visible: true,
                    scrollback: 10000,
                })),
                right: Box::new(LayoutNode::Vertical {
                    top: Box::new(LayoutNode::Pane(PaneInfo {
                        id: "llm".to_string(),
                        title: "LLM".to_string(),
                        channel_id: Some(STDLLM),
                        visible: true,
                        scrollback: 500,
                    })),
                    bottom: Box::new(LayoutNode::Tabbed {
                        tabs: vec![
                            LayoutNode::Pane(PaneInfo {
                                id: "context".to_string(),
                                title: "Context".to_string(),
                                channel_id: Some(STDCONTEXT),
                                visible: true,
                                scrollback: 1000,
                            }),
                            LayoutNode::Pane(PaneInfo {
                                id: "tool".to_string(),
                                title: "Tools".to_string(),
                                channel_id: Some(STDTOOL),
                                visible: true,
                                scrollback: 200,
                            }),
                        ],
                        active: 0,
                    }),
                    ratio: 0.6,
                }),
                ratio: 0.6,
            },
        });
    }
    
    /// Apply a preset layout
    pub fn apply_preset(&mut self, name: &str) -> Result<(), PaneError> {
        let preset = self.presets.get(name)
            .ok_or_else(|| PaneError::PresetNotFound(name.to_string()))?
            .clone();
        
        self.root = preset.layout;
        self.update_buffer_assignments();
        Ok(())
    }
    
    /// Update buffer assignment map from layout
    fn update_buffer_assignments(&mut self) {
        self.buffer_assignments.clear();
        self.collect_assignments(&self.root.clone());
    }
    
    fn collect_assignments(&mut self, node: &LayoutNode) {
        match node {
            LayoutNode::Pane(info) => {
                if let Some(channel_id) = info.channel_id {
                    self.buffer_assignments.insert(info.id.clone(), channel_id);
                }
            }
            LayoutNode::Horizontal { left, right, .. } => {
                self.collect_assignments(left);
                self.collect_assignments(right);
            }
            LayoutNode::Vertical { top, bottom, .. } => {
                self.collect_assignments(top);
                self.collect_assignments(bottom);
            }
            LayoutNode::Tabbed { tabs, .. } => {
                for tab in tabs {
                    self.collect_assignments(tab);
                }
            }
        }
    }
    
    /// Split a pane horizontally
    pub fn split_horizontal(&mut self, pane_id: &str, new_pane: PaneInfo, ratio: f32) -> Result<(), PaneError> {
        self.root = Self::split_node_horizontal(self.root.clone(), pane_id, new_pane, ratio)?;
        self.update_buffer_assignments();
        Ok(())
    }
    
    fn split_node_horizontal(node: LayoutNode, target: &str, new_pane: PaneInfo, ratio: f32) -> Result<LayoutNode, PaneError> {
        match node {
            LayoutNode::Pane(ref info) if info.id == target => {
                Ok(LayoutNode::Horizontal {
                    left: Box::new(node),
                    right: Box::new(LayoutNode::Pane(new_pane)),
                    ratio,
                })
            }
            LayoutNode::Horizontal { left, right, ratio: r } => {
                let new_left = Self::split_node_horizontal(*left, target, new_pane.clone(), ratio);
                if new_left.is_ok() {
                    return Ok(LayoutNode::Horizontal {
                        left: Box::new(new_left.unwrap()),
                        right,
                        ratio: r,
                    });
                }
                let new_right = Self::split_node_horizontal(*right, target, new_pane, ratio)?;
                Ok(LayoutNode::Horizontal {
                    left: Box::new(LayoutNode::Pane(PaneInfo::default())), // placeholder
                    right: Box::new(new_right),
                    ratio: r,
                })
            }
            LayoutNode::Vertical { top, bottom, ratio: r } => {
                let new_top = Self::split_node_horizontal(*top, target, new_pane.clone(), ratio);
                if new_top.is_ok() {
                    return Ok(LayoutNode::Vertical {
                        top: Box::new(new_top.unwrap()),
                        bottom,
                        ratio: r,
                    });
                }
                let new_bottom = Self::split_node_horizontal(*bottom, target, new_pane, ratio)?;
                Ok(LayoutNode::Vertical {
                    top: Box::new(LayoutNode::Pane(PaneInfo::default())),
                    bottom: Box::new(new_bottom),
                    ratio: r,
                })
            }
            _ => Err(PaneError::PaneNotFound(target.to_string())),
        }
    }
    
    /// Split a pane vertically
    pub fn split_vertical(&mut self, pane_id: &str, new_pane: PaneInfo, ratio: f32) -> Result<(), PaneError> {
        // Similar to split_horizontal but creates Vertical node
        self.update_buffer_assignments();
        Ok(())
    }
    
    /// Close a pane
    pub fn close_pane(&mut self, pane_id: &str) -> Result<(), PaneError> {
        // Remove pane from layout tree
        self.buffer_assignments.remove(pane_id);
        Ok(())
    }
    
    /// Focus a pane
    pub fn focus(&mut self, pane_id: &str) -> Result<(), PaneError> {
        if !self.pane_exists(pane_id) {
            return Err(PaneError::PaneNotFound(pane_id.to_string()));
        }
        self.focused = Some(pane_id.to_string());
        Ok(())
    }
    
    /// Focus next pane
    pub fn focus_next(&mut self) {
        let panes = self.list_panes();
        if panes.is_empty() {
            return;
        }
        
        let current_idx = self.focused.as_ref()
            .and_then(|f| panes.iter().position(|p| p == f))
            .unwrap_or(0);
        
        let next_idx = (current_idx + 1) % panes.len();
        self.focused = Some(panes[next_idx].clone());
    }
    
    /// Focus previous pane
    pub fn focus_prev(&mut self) {
        let panes = self.list_panes();
        if panes.is_empty() {
            return;
        }
        
        let current_idx = self.focused.as_ref()
            .and_then(|f| panes.iter().position(|p| p == f))
            .unwrap_or(0);
        
        let prev_idx = if current_idx == 0 { panes.len() - 1 } else { current_idx - 1 };
        self.focused = Some(panes[prev_idx].clone());
    }
    
    /// List all pane IDs
    pub fn list_panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        Self::collect_pane_ids(&self.root, &mut panes);
        panes
    }
    
    fn collect_pane_ids(node: &LayoutNode, panes: &mut Vec<PaneId>) {
        match node {
            LayoutNode::Pane(info) => panes.push(info.id.clone()),
            LayoutNode::Horizontal { left, right, .. } => {
                Self::collect_pane_ids(left, panes);
                Self::collect_pane_ids(right, panes);
            }
            LayoutNode::Vertical { top, bottom, .. } => {
                Self::collect_pane_ids(top, panes);
                Self::collect_pane_ids(bottom, panes);
            }
            LayoutNode::Tabbed { tabs, .. } => {
                for tab in tabs {
                    Self::collect_pane_ids(tab, panes);
                }
            }
        }
    }
    
    fn pane_exists(&self, pane_id: &str) -> bool {
        self.list_panes().contains(&pane_id.to_string())
    }
    
    /// Get currently focused pane
    pub fn focused(&self) -> Option<&PaneId> {
        self.focused.as_ref()
    }
    
    /// Get root layout for rendering
    pub fn layout(&self) -> &LayoutNode {
        &self.root
    }
    
    /// Resize split ratio
    pub fn resize(&mut self, pane_id: &str, delta: f32) -> Result<(), PaneError> {
        // Adjust ratio of split containing this pane
        Ok(())
    }
}

impl Default for PaneInfo {
    fn default() -> Self {
        Self {
            id: generate_id("pane"),
            title: "Pane".to_string(),
            channel_id: None,
            visible: true,
            scrollback: 10000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PaneError {
    #[error("Pane not found: {0}")]
    PaneNotFound(String),
    #[error("Preset not found: {0}")]
    PresetNotFound(String),
    #[error("Invalid layout operation")]
    InvalidOperation,
}
```

## 8.4 Buffer Persistence

```rust
// crates/colony-channels/src/buffer/storage.rs

use std::path::PathBuf;
use std::fs;

/// Manages buffer persistence to disk
pub struct BufferStorage {
    base_path: PathBuf,
    session_id: String,
}

impl BufferStorage {
    pub fn new(session_id: &str) -> Self {
        let base = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("colony")
            .join("buffers");
        
        Self {
            base_path: base,
            session_id: session_id.to_string(),
        }
    }
    
    /// Get path for a buffer file
    fn buffer_path(&self, channel_id: ChannelId) -> PathBuf {
        self.base_path
            .join(&self.session_id)
            .join(format!("channel_{}.log", channel_id))
    }
    
    /// Save buffer to disk
    pub fn save(&self, channel_id: ChannelId, buffer: &Buffer) -> io::Result<()> {
        let path = self.buffer_path(channel_id);
        
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Write buffer content
        let content = buffer.to_plain_text();
        fs::write(&path, content)?;
        
        // Also save metadata
        let meta_path = path.with_extension("meta.json");
        let metadata = BufferMetadata {
            channel_id,
            name: buffer.name.clone(),
            line_count: buffer.len(),
            last_modified: buffer.last_modified,
        };
        let meta_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&meta_path, meta_json)?;
        
        Ok(())
    }
    
    /// Load buffer from disk
    pub fn load(&self, channel_id: ChannelId) -> io::Result<Buffer> {
        let path = self.buffer_path(channel_id);
        
        let content = fs::read_to_string(&path)?;
        
        // Load metadata
        let meta_path = path.with_extension("meta.json");
        let metadata: BufferMetadata = if meta_path.exists() {
            let meta_json = fs::read_to_string(&meta_path)?;
            serde_json::from_str(&meta_json)?
        } else {
            BufferMetadata {
                channel_id,
                name: format!("channel_{}", channel_id),
                line_count: 0,
                last_modified: Utc::now(),
            }
        };
        
        let mut buffer = Buffer::new(channel_id, &metadata.name);
        for line in content.lines() {
            buffer.append_line(line, LineStyle::default());
        }
        
        Ok(buffer)
    }
    
    /// List saved sessions
    pub fn list_sessions(&self) -> io::Result<Vec<String>> {
        let mut sessions = Vec::new();
        
        if self.base_path.exists() {
            for entry in fs::read_dir(&self.base_path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        sessions.push(name.to_string());
                    }
                }
            }
        }
        
        Ok(sessions)
    }
    
    /// Delete old sessions
    pub fn cleanup_old(&self, max_age_days: u64) -> io::Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);
        let mut deleted = 0;
        
        if self.base_path.exists() {
            for entry in fs::read_dir(&self.base_path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                
                if let Ok(modified) = metadata.modified() {
                    let modified: DateTime<Utc> = modified.into();
                    if modified < cutoff {
                        fs::remove_dir_all(entry.path())?;
                        deleted += 1;
                    }
                }
            }
        }
        
        Ok(deleted)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BufferMetadata {
    channel_id: ChannelId,
    name: String,
    line_count: usize,
    last_modified: DateTime<Utc>,
}
```

## 8.5 Pane Commands

```colony
# Apply preset layout
pane:layout default           # Single main pane
pane:layout dev               # Main + context + llm
pane:layout debug             # Full debug layout
pane:layout agent             # Agent development layout

# List presets
pane:presets

# Split current pane
pane:split-h                  # Horizontal split (left/right)
pane:split-v                  # Vertical split (top/bottom)
pane:split-h --ratio 0.3      # Custom ratio

# Assign channel to pane
pane:assign context ctx       # Show context channel in 'context' pane
pane:assign tool stderr       # Show stderr in 'tool' pane

# Focus management
pane:focus main               # Focus specific pane
pane:focus-next               # Cycle to next pane (Ctrl+w n)
pane:focus-prev               # Cycle to previous pane (Ctrl+w p)

# Resize
pane:resize +5                # Increase size
pane:resize -5                # Decrease size

# Close
pane:close                    # Close current pane
pane:close context            # Close specific pane

# Zoom
pane:zoom                     # Toggle zoom on current pane
pane:zoom main                # Zoom specific pane

# Scroll
pane:scroll-up 10             # Scroll up 10 lines
pane:scroll-down 10           # Scroll down
pane:scroll-top               # Go to top
pane:scroll-bottom            # Go to bottom

# Search
pane:search "pattern"         # Search in current pane
pane:search-next              # Next match
pane:search-prev              # Previous match

# Save/restore
pane:save-layout "my-layout"  # Save current layout
pane:load-layout "my-layout"  # Load saved layout

# Buffer operations
buffer:clear                  # Clear current buffer
buffer:clear context          # Clear specific buffer
buffer:save > output.log      # Save buffer to file
buffer:export --ansi > out.txt  # Export with colors
```

## 8.6 Keyboard Shortcuts

```rust
// Default pane keybindings

pub fn default_pane_keybindings() -> Vec<KeyBinding> {
    vec![
        // Focus
        KeyBinding::new("ctrl+w n", PaneAction::FocusNext),
        KeyBinding::new("ctrl+w p", PaneAction::FocusPrev),
        KeyBinding::new("ctrl+w h", PaneAction::FocusLeft),
        KeyBinding::new("ctrl+w j", PaneAction::FocusDown),
        KeyBinding::new("ctrl+w k", PaneAction::FocusUp),
        KeyBinding::new("ctrl+w l", PaneAction::FocusRight),
        
        // Split
        KeyBinding::new("ctrl+w v", PaneAction::SplitVertical),
        KeyBinding::new("ctrl+w s", PaneAction::SplitHorizontal),
        
        // Resize
        KeyBinding::new("ctrl+w +", PaneAction::ResizeIncrease),
        KeyBinding::new("ctrl+w -", PaneAction::ResizeDecrease),
        KeyBinding::new("ctrl+w =", PaneAction::ResizeEqual),
        
        // Close
        KeyBinding::new("ctrl+w c", PaneAction::Close),
        KeyBinding::new("ctrl+w o", PaneAction::CloseOthers),
        
        // Zoom
        KeyBinding::new("ctrl+w z", PaneAction::ToggleZoom),
        
        // Scroll
        KeyBinding::new("ctrl+shift+up", PaneAction::ScrollUp(1)),
        KeyBinding::new("ctrl+shift+down", PaneAction::ScrollDown(1)),
        KeyBinding::new("ctrl+shift+pageup", PaneAction::ScrollUp(10)),
        KeyBinding::new("ctrl+shift+pagedown", PaneAction::ScrollDown(10)),
        KeyBinding::new("ctrl+shift+home", PaneAction::ScrollTop),
        KeyBinding::new("ctrl+shift+end", PaneAction::ScrollBottom),
        
        // Layout presets
        KeyBinding::new("ctrl+w 1", PaneAction::ApplyPreset("default")),
        KeyBinding::new("ctrl+w 2", PaneAction::ApplyPreset("dev")),
        KeyBinding::new("ctrl+w 3", PaneAction::ApplyPreset("debug")),
        KeyBinding::new("ctrl+w 4", PaneAction::ApplyPreset("agent")),
    ]
}

pub struct KeyBinding {
    pub keys: String,
    pub action: PaneAction,
}

pub enum PaneAction {
    FocusNext,
    FocusPrev,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    SplitVertical,
    SplitHorizontal,
    ResizeIncrease,
    ResizeDecrease,
    ResizeEqual,
    Close,
    CloseOthers,
    ToggleZoom,
    ScrollUp(usize),
    ScrollDown(usize),
    ScrollTop,
    ScrollBottom,
    ApplyPreset(&'static str),
}
```

---

*Continue to Part 9: Agent System*
