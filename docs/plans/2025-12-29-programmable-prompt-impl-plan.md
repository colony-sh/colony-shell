# Programmable Prompt Component System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Warp-style programmable prompt system where users compose prompts from built-in and custom components that react to tuple space events.

**Architecture:** New `colony-ui-core` crate provides `PromptComponent` trait, core types (`Style`, `StyledSpan`, `ComponentOutput`), 7 built-in components, `SubscriptionManager` for cache invalidation, and ANSI renderer. Integration into REPL replaces static prompt with dynamic component-based prompt.

**Tech Stack:** Rust, serde, ANSI escape codes, integration with existing `colony-tuple`, `colony-protocol`, `colony-config`, `colony-exec`

---

## Task 1: Create colony-ui-core Crate Skeleton

**Files:**
- Create: `crates/colony-ui-core/Cargo.toml`
- Create: `crates/colony-ui-core/src/lib.rs`
- Modify: `Cargo.toml:18` (add to workspace members)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "colony-ui-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Programmable prompt component system for Colony Shell"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
colony-tuple = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
```

**Step 2: Create lib.rs skeleton**

```rust
//! Programmable prompt component system for Colony Shell
//!
//! Provides a component-based prompt where users compose prompts from
//! built-in and custom components that react to tuple space events.

pub mod component;

pub use component::{ComponentOutput, PromptComponent, Style, StyledSpan};
```

**Step 3: Add to workspace**

In `Cargo.toml`, add `"crates/colony-ui-core"` to the workspace members array after `"crates/colony-tree-sitter"`.

**Step 4: Verify compilation**

Run: `cargo check -p colony-ui-core`
Expected: Success (empty crate compiles)

**Step 5: Commit**

```bash
git add crates/colony-ui-core/ Cargo.toml
git commit -m "feat(ui-core): create colony-ui-core crate skeleton"
```

---

## Task 2: Implement Core Types (Style, StyledSpan, ComponentOutput)

**Files:**
- Create: `crates/colony-ui-core/src/component.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for core types**

```rust
// crates/colony-ui-core/src/component.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_default() {
        let style = Style::default();
        assert_eq!(style.fg, None);
        assert_eq!(style.bg, None);
        assert!(!style.bold);
        assert!(!style.italic);
        assert!(!style.dim);
    }

    #[test]
    fn test_styled_span_creation() {
        let span = StyledSpan::new("hello", Style::default());
        assert_eq!(span.text, "hello");
    }

    #[test]
    fn test_styled_span_with_style() {
        let span = StyledSpan::new("hello", Style::default().fg(Color::Cyan).bold());
        assert_eq!(span.style.fg, Some(Color::Cyan));
        assert!(span.style.bold);
    }

    #[test]
    fn test_component_output_visible() {
        let output = ComponentOutput::visible(vec![
            StyledSpan::new("test", Style::default())
        ]);
        assert!(output.visible);
        assert_eq!(output.spans.len(), 1);
    }

    #[test]
    fn test_component_output_hidden() {
        let output = ComponentOutput::hidden();
        assert!(!output.visible);
        assert!(output.spans.is_empty());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL - types not defined yet

**Step 3: Implement core types**

```rust
//! Core component types for the prompt system

use serde::{Deserialize, Serialize};

/// ANSI color values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    /// RGB color (256-color or truecolor)
    Rgb(u8, u8, u8),
}

impl Color {
    /// Convert to ANSI foreground escape code number
    pub fn to_ansi_fg(&self) -> String {
        match self {
            Color::Black => "30".to_string(),
            Color::Red => "31".to_string(),
            Color::Green => "32".to_string(),
            Color::Yellow => "33".to_string(),
            Color::Blue => "34".to_string(),
            Color::Magenta => "35".to_string(),
            Color::Cyan => "36".to_string(),
            Color::White => "37".to_string(),
            Color::BrightBlack => "90".to_string(),
            Color::BrightRed => "91".to_string(),
            Color::BrightGreen => "92".to_string(),
            Color::BrightYellow => "93".to_string(),
            Color::BrightBlue => "94".to_string(),
            Color::BrightMagenta => "95".to_string(),
            Color::BrightCyan => "96".to_string(),
            Color::BrightWhite => "97".to_string(),
            Color::Rgb(r, g, b) => format!("38;2;{};{};{}", r, g, b),
        }
    }

    /// Convert to ANSI background escape code number
    pub fn to_ansi_bg(&self) -> String {
        match self {
            Color::Black => "40".to_string(),
            Color::Red => "41".to_string(),
            Color::Green => "42".to_string(),
            Color::Yellow => "43".to_string(),
            Color::Blue => "44".to_string(),
            Color::Magenta => "45".to_string(),
            Color::Cyan => "46".to_string(),
            Color::White => "47".to_string(),
            Color::BrightBlack => "100".to_string(),
            Color::BrightRed => "101".to_string(),
            Color::BrightGreen => "102".to_string(),
            Color::BrightYellow => "103".to_string(),
            Color::BrightBlue => "104".to_string(),
            Color::BrightMagenta => "105".to_string(),
            Color::BrightCyan => "106".to_string(),
            Color::BrightWhite => "107".to_string(),
            Color::Rgb(r, g, b) => format!("48;2;{};{};{}", r, g, b),
        }
    }
}

/// Style for a span of text
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
}

impl Style {
    /// Set foreground color
    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Set background color
    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    /// Set bold
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Set italic
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Set dim
    pub fn dim(mut self) -> Self {
        self.dim = true;
        self
    }
}

/// A styled piece of text
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

impl StyledSpan {
    /// Create a new styled span
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    /// Create a plain (unstyled) span
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
        }
    }
}

/// Output from a component render
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComponentOutput {
    /// Text representation with styling (always present)
    pub spans: Vec<StyledSpan>,
    /// Semantic data for rich renderers (optional)
    pub data: Option<serde_json::Value>,
    /// Should this component be hidden? (e.g., exit_status when 0)
    pub visible: bool,
}

impl ComponentOutput {
    /// Create a visible output with spans
    pub fn visible(spans: Vec<StyledSpan>) -> Self {
        Self {
            spans,
            data: None,
            visible: true,
        }
    }

    /// Create a visible output with spans and semantic data
    pub fn with_data(spans: Vec<StyledSpan>, data: serde_json::Value) -> Self {
        Self {
            spans,
            data: Some(data),
            visible: true,
        }
    }

    /// Create a hidden output (component has nothing to show)
    pub fn hidden() -> Self {
        Self {
            spans: vec![],
            data: None,
            visible: false,
        }
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS - all tests pass

**Step 5: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement Style, StyledSpan, ComponentOutput types"
```

---

## Task 3: Implement ComponentContext and PromptComponent Trait

**Files:**
- Create: `crates/colony-ui-core/src/context.rs`
- Modify: `crates/colony-ui-core/src/component.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`
- Modify: `crates/colony-ui-core/Cargo.toml`

**Step 1: Add required dependencies**

In `crates/colony-ui-core/Cargo.toml`, add:
```toml
[dependencies]
# ... existing deps
colony-exec = { workspace = true }
```

**Step 2: Write tests for context**

```rust
// crates/colony-ui-core/src/context.rs

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_context_creation() {
        let ctx = ComponentContext {
            cwd: std::path::PathBuf::from("/home/user"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        };
        assert_eq!(ctx.cwd.to_str(), Some("/home/user"));
        assert_eq!(ctx.last_exit_code, 0);
    }

    #[test]
    fn test_context_with_model() {
        let ctx = ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: Some("gpt-4".to_string()),
        };
        assert_eq!(ctx.model, Some("gpt-4".to_string()));
    }
}
```

**Step 3: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL - context not defined yet

**Step 4: Implement ComponentContext**

```rust
//! Component context - data passed to components during render

use std::collections::HashMap;
use std::path::PathBuf;

/// Context passed to components during render
///
/// This provides all data components might need without reaching into global state.
#[derive(Debug, Clone)]
pub struct ComponentContext {
    /// Current working directory
    pub cwd: PathBuf,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Last command exit code
    pub last_exit_code: i32,
    /// Number of running background jobs
    pub running_jobs: usize,
    /// Number of stopped jobs
    pub stopped_jobs: usize,
    /// Current AI model (if agent active)
    pub model: Option<String>,
}

impl ComponentContext {
    /// Create context from executor state
    pub fn from_executor(
        executor: &colony_exec::Executor,
        model: Option<String>,
    ) -> Self {
        let jobs = executor.jobs().list_jobs();
        let running = jobs.iter().filter(|j| j.is_running()).count();
        let stopped = jobs.iter().filter(|j| j.is_stopped()).count();

        Self {
            cwd: std::env::current_dir().unwrap_or_default(),
            env: std::env::vars().collect(),
            last_exit_code: executor.context().last_exit_status(),
            running_jobs: running,
            stopped_jobs: stopped,
            model,
        }
    }

    /// Get an environment variable
    pub fn get_env(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(|s| s.as_str())
    }

    /// Get home directory (from HOME env var)
    pub fn home_dir(&self) -> Option<&str> {
        self.get_env("HOME")
    }
}
```

**Step 5: Add PromptComponent trait to component.rs**

```rust
// Add to end of crates/colony-ui-core/src/component.rs

use crate::context::ComponentContext;
use colony_tuple::Value;

/// Tuple patterns a component subscribes to
pub type TuplePattern = Vec<Value>;

/// The trait all prompt components implement
pub trait PromptComponent: Send + Sync {
    /// Unique identifier (e.g., "git_branch")
    fn id(&self) -> &str;

    /// Tuple patterns this component reacts to
    fn subscriptions(&self) -> Vec<TuplePattern>;

    /// Render current state
    fn render(&self, ctx: &ComponentContext) -> ComponentOutput;
}
```

**Step 6: Update lib.rs exports**

```rust
//! Programmable prompt component system for Colony Shell

pub mod component;
pub mod context;

pub use component::{
    Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern,
};
pub use context::ComponentContext;
```

**Step 7: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 8: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): add ComponentContext and PromptComponent trait"
```

---

## Task 4: Implement ANSI Renderer

**Files:**
- Create: `crates/colony-ui-core/src/render/mod.rs`
- Create: `crates/colony-ui-core/src/render/ansi.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for ANSI renderer**

```rust
// crates/colony-ui-core/src/render/ansi.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, ComponentOutput, Style, StyledSpan};

    #[test]
    fn test_render_plain_span() {
        let span = StyledSpan::plain("hello");
        let result = span_to_ansi(&span);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_render_colored_span() {
        let span = StyledSpan::new("hello", Style::default().fg(Color::Cyan));
        let result = span_to_ansi(&span);
        assert!(result.contains("\x1b[36m")); // Cyan
        assert!(result.contains("hello"));
        assert!(result.contains("\x1b[0m")); // Reset
    }

    #[test]
    fn test_render_bold_span() {
        let span = StyledSpan::new("hello", Style::default().bold());
        let result = span_to_ansi(&span);
        assert!(result.contains("\x1b[1m")); // Bold
    }

    #[test]
    fn test_strip_ansi() {
        let text = "\x1b[36mhello\x1b[0m";
        let result = strip_ansi(text);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_render_outputs_skips_hidden() {
        let outputs = vec![
            ComponentOutput::visible(vec![StyledSpan::plain("visible")]),
            ComponentOutput::hidden(),
            ComponentOutput::visible(vec![StyledSpan::plain("also visible")]),
        ];
        let result = render_outputs(&outputs);
        assert!(result.contains("visible"));
        assert!(result.contains("also visible"));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL - renderer not implemented

**Step 3: Implement ANSI renderer**

```rust
//! ANSI escape code renderer for terminal output

use crate::{ComponentOutput, Style, StyledSpan};

/// Render a single span to ANSI escape codes
pub fn span_to_ansi(span: &StyledSpan) -> String {
    let mut codes = vec![];

    if let Some(ref fg) = span.style.fg {
        codes.push(fg.to_ansi_fg());
    }
    if let Some(ref bg) = span.style.bg {
        codes.push(bg.to_ansi_bg());
    }
    if span.style.bold {
        codes.push("1".to_string());
    }
    if span.style.dim {
        codes.push("2".to_string());
    }
    if span.style.italic {
        codes.push("3".to_string());
    }

    if codes.is_empty() {
        span.text.clone()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), span.text)
    }
}

/// Render multiple spans to a single ANSI string
pub fn spans_to_ansi(spans: &[StyledSpan]) -> String {
    spans.iter().map(span_to_ansi).collect()
}

/// Render component outputs to ANSI string (filters hidden components)
pub fn render_outputs(outputs: &[ComponentOutput]) -> String {
    outputs
        .iter()
        .filter(|o| o.visible)
        .map(|o| spans_to_ansi(&o.spans))
        .collect::<Vec<_>>()
        .join("  ") // Gap between components
}

/// Render full prompt with left and right components
///
/// Right components are right-aligned based on terminal width.
pub fn render_prompt(
    left: &[ComponentOutput],
    right: &[ComponentOutput],
    width: usize,
) -> String {
    let left_str = render_outputs(left);
    let right_str = render_outputs(right);

    let left_visible_len = strip_ansi(&left_str).len();
    let right_visible_len = strip_ansi(&right_str).len();

    // Calculate padding to right-align right components
    // Reserve 2 chars for "❯ " prompt symbol and some buffer
    let min_gap = 2;
    let padding = width
        .saturating_sub(left_visible_len)
        .saturating_sub(right_visible_len)
        .saturating_sub(min_gap);

    if right_str.is_empty() {
        format!("{}\n❯ ", left_str)
    } else {
        format!("{}{:padding$}{}\n❯ ", left_str, "", right_str, padding = padding)
    }
}

/// Strip ANSI escape codes from a string
pub fn strip_ansi(s: &str) -> String {
    // Simple regex-free approach: skip escape sequences
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until 'm' (SGR terminator)
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}
```

**Step 4: Create render/mod.rs**

```rust
//! Rendering utilities for prompt components

pub mod ansi;

pub use ansi::{render_outputs, render_prompt, span_to_ansi, spans_to_ansi, strip_ansi};
```

**Step 5: Update lib.rs**

```rust
//! Programmable prompt component system for Colony Shell

pub mod component;
pub mod context;
pub mod render;

pub use component::{
    Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern,
};
pub use context::ComponentContext;
pub use render::ansi::{render_outputs, render_prompt, strip_ansi};
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement ANSI renderer"
```

---

## Task 5: Implement Built-in Components - pwd

**Files:**
- Create: `crates/colony-ui-core/src/builtins/mod.rs`
- Create: `crates/colony-ui-core/src/builtins/pwd.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for pwd component**

```rust
// crates/colony-ui-core/src/builtins/pwd.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentContext;
    use std::collections::HashMap;

    fn test_context(cwd: &str, home: Option<&str>) -> ComponentContext {
        let mut env = HashMap::new();
        if let Some(h) = home {
            env.insert("HOME".to_string(), h.to_string());
        }
        ComponentContext {
            cwd: std::path::PathBuf::from(cwd),
            env,
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        }
    }

    #[test]
    fn test_pwd_in_home() {
        let pwd = PwdComponent::new();
        let ctx = test_context("/Users/test", Some("/Users/test"));
        let output = pwd.render(&ctx);
        
        assert!(output.visible);
        assert_eq!(output.spans[0].text, "~");
    }

    #[test]
    fn test_pwd_in_home_subdir() {
        let pwd = PwdComponent::new();
        let ctx = test_context("/Users/test/projects", Some("/Users/test"));
        let output = pwd.render(&ctx);
        
        assert!(output.visible);
        assert_eq!(output.spans[0].text, "~/projects");
    }

    #[test]
    fn test_pwd_outside_home() {
        let pwd = PwdComponent::new();
        let ctx = test_context("/var/log", Some("/Users/test"));
        let output = pwd.render(&ctx);
        
        assert!(output.visible);
        assert_eq!(output.spans[0].text, "/var/log");
    }

    #[test]
    fn test_pwd_truncation() {
        let pwd = PwdComponent::with_max_depth(3);
        let ctx = test_context("/a/b/c/d/e/f", None);
        let output = pwd.render(&ctx);
        
        // Should show ...d/e/f (last 3 components)
        assert!(output.spans[0].text.starts_with("..."));
        assert!(output.spans[0].text.ends_with("d/e/f"));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement pwd component**

```rust
//! pwd component - shows current working directory

use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

/// pwd component configuration
pub struct PwdComponent {
    /// Maximum path depth before truncation (0 = no limit)
    max_depth: usize,
}

impl PwdComponent {
    /// Create with default settings
    pub fn new() -> Self {
        Self { max_depth: 0 }
    }

    /// Create with max depth truncation
    pub fn with_max_depth(max_depth: usize) -> Self {
        Self { max_depth }
    }

    /// Truncate path to last N components
    fn truncate_path(&self, path: &str) -> String {
        if self.max_depth == 0 {
            return path.to_string();
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        
        if components.len() <= self.max_depth {
            return path.to_string();
        }

        let truncated: Vec<&str> = components
            .iter()
            .rev()
            .take(self.max_depth)
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        format!("...{}", truncated.join("/"))
    }
}

impl Default for PwdComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for PwdComponent {
    fn id(&self) -> &str {
        "pwd"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![Value::Str("dir_change".to_string()), Value::Wildcard, Value::Wildcard]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let cwd = ctx.cwd.to_string_lossy();
        
        // Replace home directory with ~
        let display_path = if let Some(home) = ctx.home_dir() {
            if cwd.starts_with(home) {
                if cwd.len() == home.len() {
                    "~".to_string()
                } else {
                    format!("~{}", &cwd[home.len()..])
                }
            } else {
                cwd.to_string()
            }
        } else {
            cwd.to_string()
        };

        let display_path = self.truncate_path(&display_path);

        let spans = vec![StyledSpan::new(
            display_path.clone(),
            Style::default().fg(Color::Cyan),
        )];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "full": cwd,
                "display": display_path,
            }),
        )
    }
}
```

**Step 4: Create builtins/mod.rs**

```rust
//! Built-in prompt components

pub mod pwd;

pub use pwd::PwdComponent;
```

**Step 5: Update lib.rs**

```rust
//! Programmable prompt component system for Colony Shell

pub mod builtins;
pub mod component;
pub mod context;
pub mod render;

pub use builtins::PwdComponent;
pub use component::{
    Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern,
};
pub use context::ComponentContext;
pub use render::ansi::{render_outputs, render_prompt, strip_ansi};
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement pwd component"
```

---

## Task 6: Implement Built-in Components - exit_status

**Files:**
- Create: `crates/colony-ui-core/src/builtins/exit_status.rs`
- Modify: `crates/colony-ui-core/src/builtins/mod.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for exit_status component**

```rust
// crates/colony-ui-core/src/builtins/exit_status.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentContext;
    use std::collections::HashMap;

    fn test_context(exit_code: i32) -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: exit_code,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        }
    }

    #[test]
    fn test_exit_status_zero_hidden() {
        let component = ExitStatusComponent::new();
        let ctx = test_context(0);
        let output = component.render(&ctx);
        
        assert!(!output.visible);
    }

    #[test]
    fn test_exit_status_nonzero_visible() {
        let component = ExitStatusComponent::new();
        let ctx = test_context(1);
        let output = component.render(&ctx);
        
        assert!(output.visible);
        assert!(output.spans[0].text.contains("1"));
    }

    #[test]
    fn test_exit_status_127_not_found() {
        let component = ExitStatusComponent::new();
        let ctx = test_context(127);
        let output = component.render(&ctx);
        
        assert!(output.visible);
        assert!(output.spans[0].text.contains("127"));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement exit_status component**

```rust
//! exit_status component - shows last command exit code when non-zero

use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

/// exit_status component
pub struct ExitStatusComponent;

impl ExitStatusComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExitStatusComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for ExitStatusComponent {
    fn id(&self) -> &str {
        "exit_status"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![Value::Str("cmd_complete".to_string()), Value::Wildcard]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        if ctx.last_exit_code == 0 {
            return ComponentOutput::hidden();
        }

        let text = format!("✘ {}", ctx.last_exit_code);
        let spans = vec![StyledSpan::new(
            text,
            Style::default().fg(Color::Red).bold(),
        )];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "code": ctx.last_exit_code,
            }),
        )
    }
}
```

**Step 4: Update builtins/mod.rs**

```rust
//! Built-in prompt components

pub mod exit_status;
pub mod pwd;

pub use exit_status::ExitStatusComponent;
pub use pwd::PwdComponent;
```

**Step 5: Update lib.rs exports**

```rust
pub use builtins::{ExitStatusComponent, PwdComponent};
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement exit_status component"
```

---

## Task 7: Implement Built-in Components - jobs

**Files:**
- Create: `crates/colony-ui-core/src/builtins/jobs.rs`
- Modify: `crates/colony-ui-core/src/builtins/mod.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for jobs component**

```rust
// crates/colony-ui-core/src/builtins/jobs.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentContext;
    use std::collections::HashMap;

    fn test_context(running: usize, stopped: usize) -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: running,
            stopped_jobs: stopped,
            model: None,
        }
    }

    #[test]
    fn test_jobs_none_hidden() {
        let component = JobsComponent::new();
        let ctx = test_context(0, 0);
        let output = component.render(&ctx);
        
        assert!(!output.visible);
    }

    #[test]
    fn test_jobs_running() {
        let component = JobsComponent::new();
        let ctx = test_context(2, 0);
        let output = component.render(&ctx);
        
        assert!(output.visible);
        assert!(output.spans[0].text.contains("2"));
    }

    #[test]
    fn test_jobs_stopped() {
        let component = JobsComponent::new();
        let ctx = test_context(0, 1);
        let output = component.render(&ctx);
        
        assert!(output.visible);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement jobs component**

```rust
//! jobs component - shows background/stopped job count

use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

/// jobs component
pub struct JobsComponent;

impl JobsComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JobsComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for JobsComponent {
    fn id(&self) -> &str {
        "jobs"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![Value::Str("job_change".to_string()), Value::Wildcard]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let total = ctx.running_jobs + ctx.stopped_jobs;
        
        if total == 0 {
            return ComponentOutput::hidden();
        }

        let text = format!("⚙ {}", total);
        let color = if ctx.stopped_jobs > 0 {
            Color::Red // Red if any stopped jobs
        } else {
            Color::Yellow // Yellow for running
        };

        let spans = vec![StyledSpan::new(text, Style::default().fg(color))];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "running": ctx.running_jobs,
                "stopped": ctx.stopped_jobs,
            }),
        )
    }
}
```

**Step 4: Update builtins/mod.rs**

```rust
//! Built-in prompt components

pub mod exit_status;
pub mod jobs;
pub mod pwd;

pub use exit_status::ExitStatusComponent;
pub use jobs::JobsComponent;
pub use pwd::PwdComponent;
```

**Step 5: Update lib.rs exports**

```rust
pub use builtins::{ExitStatusComponent, JobsComponent, PwdComponent};
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement jobs component"
```

---

## Task 8: Implement Built-in Components - git_branch

**Files:**
- Create: `crates/colony-ui-core/src/builtins/git_branch.rs`
- Modify: `crates/colony-ui-core/src/builtins/mod.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for git_branch component**

```rust
// crates/colony-ui-core/src/builtins/git_branch.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_branch_normal() {
        let output = "* main\n  feature-x\n";
        let result = parse_git_branch(output);
        assert_eq!(result, Some(("main".to_string(), false)));
    }

    #[test]
    fn test_parse_git_branch_detached() {
        let output = "* (HEAD detached at abc1234)\n  main\n";
        let result = parse_git_branch(output);
        assert_eq!(result, Some(("abc1234".to_string(), true)));
    }

    #[test]
    fn test_parse_git_branch_empty() {
        let output = "";
        let result = parse_git_branch(output);
        assert_eq!(result, None);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement git_branch component**

```rust
//! git_branch component - shows current git branch

use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use std::process::Command;

/// git_branch component
pub struct GitBranchComponent;

impl GitBranchComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitBranchComponent {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse git branch output to extract branch name
pub fn parse_git_branch(output: &str) -> Option<(String, bool)> {
    for line in output.lines() {
        if line.starts_with('*') {
            let branch_part = line[1..].trim();
            
            // Check for detached HEAD
            if branch_part.starts_with("(HEAD detached at ") {
                let hash = branch_part
                    .strip_prefix("(HEAD detached at ")
                    .and_then(|s| s.strip_suffix(')'))
                    .unwrap_or(branch_part);
                return Some((hash.to_string(), true));
            }
            
            // Check for other parenthesized states
            if branch_part.starts_with('(') {
                // Rebasing, merging, etc.
                return Some((branch_part.to_string(), true));
            }
            
            return Some((branch_part.to_string(), false));
        }
    }
    None
}

impl PromptComponent for GitBranchComponent {
    fn id(&self) -> &str {
        "git_branch"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![
            Value::Str("dir_change".to_string()),
            Value::Wildcard,
            Value::Wildcard,
        ]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        // Run git branch to get current branch
        let output = Command::new("git")
            .arg("branch")
            .current_dir(&ctx.cwd)
            .output();

        let (branch, detached) = match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                match parse_git_branch(&stdout) {
                    Some((b, d)) => (b, d),
                    None => return ComponentOutput::hidden(),
                }
            }
            _ => return ComponentOutput::hidden(),
        };

        let icon = "";
        let text = format!("{} {}", icon, branch);
        
        let color = if detached {
            Color::Yellow // Warning color for detached HEAD
        } else {
            Color::Green
        };

        let spans = vec![StyledSpan::new(text, Style::default().fg(color))];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "branch": branch,
                "detached": detached,
            }),
        )
    }
}
```

**Step 4: Update builtins/mod.rs**

```rust
//! Built-in prompt components

pub mod exit_status;
pub mod git_branch;
pub mod jobs;
pub mod pwd;

pub use exit_status::ExitStatusComponent;
pub use git_branch::GitBranchComponent;
pub use jobs::JobsComponent;
pub use pwd::PwdComponent;
```

**Step 5: Update lib.rs exports**

```rust
pub use builtins::{ExitStatusComponent, GitBranchComponent, JobsComponent, PwdComponent};
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement git_branch component"
```

---

## Task 9: Implement Built-in Components - git_status

**Files:**
- Create: `crates/colony-ui-core/src/builtins/git_status.rs`
- Modify: `crates/colony-ui-core/src/builtins/mod.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for git_status component**

```rust
// crates/colony-ui-core/src/builtins/git_status.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff_stat_summary() {
        let output = " file1.rs |  10 ++++------
 file2.rs |   5 +++++
 3 files changed, 12 insertions(+), 6 deletions(-)
";
        let result = parse_diff_stat(output);
        assert_eq!(result.files_changed, 3);
        assert_eq!(result.insertions, 12);
        assert_eq!(result.deletions, 6);
    }

    #[test]
    fn test_parse_diff_stat_empty() {
        let output = "";
        let result = parse_diff_stat(output);
        assert_eq!(result.files_changed, 0);
    }

    #[test]
    fn test_parse_porcelain_status() {
        let output = " M file1.rs
?? newfile.rs
A  staged.rs
";
        let result = parse_porcelain_status(output);
        assert_eq!(result.modified, 1);
        assert_eq!(result.untracked, 1);
        assert_eq!(result.staged, 1);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement git_status component**

```rust
//! git_status component - shows git diff stats and status

use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;
use std::process::Command;

/// git_status component
pub struct GitStatusComponent;

impl GitStatusComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitStatusComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct DiffStat {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Debug, Default)]
pub struct StatusInfo {
    pub modified: usize,
    pub staged: usize,
    pub untracked: usize,
}

/// Parse git diff --stat output for summary line
pub fn parse_diff_stat(output: &str) -> DiffStat {
    let mut stat = DiffStat::default();
    
    // Look for summary line: "N files changed, X insertions(+), Y deletions(-)"
    for line in output.lines() {
        if line.contains("files changed") || line.contains("file changed") {
            // Parse insertions
            if let Some(ins_match) = line.split("insertion").next() {
                if let Some(num) = ins_match.split_whitespace().last() {
                    stat.insertions = num.parse().unwrap_or(0);
                }
            }
            // Parse deletions
            if let Some(del_part) = line.split("deletion").next() {
                if let Some(num) = del_part.split(',').last().and_then(|s| s.split_whitespace().last()) {
                    stat.deletions = num.parse().unwrap_or(0);
                }
            }
            // Parse files changed
            if let Some(num) = line.split_whitespace().next() {
                stat.files_changed = num.parse().unwrap_or(0);
            }
        }
    }
    
    stat
}

/// Parse git status --porcelain output
pub fn parse_porcelain_status(output: &str) -> StatusInfo {
    let mut info = StatusInfo::default();
    
    for line in output.lines() {
        if line.len() < 2 {
            continue;
        }
        
        let index = line.chars().next().unwrap_or(' ');
        let worktree = line.chars().nth(1).unwrap_or(' ');
        
        match (index, worktree) {
            ('?', '?') => info.untracked += 1,
            ('A', _) | ('M', ' ') | ('D', ' ') | ('R', _) | ('C', _) => info.staged += 1,
            (_, 'M') | (_, 'D') => info.modified += 1,
            _ => {}
        }
    }
    
    info
}

impl PromptComponent for GitStatusComponent {
    fn id(&self) -> &str {
        "git_status"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![
            vec![Value::Str("dir_change".to_string()), Value::Wildcard, Value::Wildcard],
            vec![Value::Str("file_change".to_string()), Value::Wildcard],
        ]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        // Check if we're in a git repo first
        let in_git = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(&ctx.cwd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !in_git {
            return ComponentOutput::hidden();
        }

        // Get status info
        let status_output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&ctx.cwd)
            .output();

        let status_info = match status_output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                parse_porcelain_status(&stdout)
            }
            _ => StatusInfo::default(),
        };

        // Get diff stats
        let diff_output = Command::new("git")
            .args(["diff", "--stat", "HEAD"])
            .current_dir(&ctx.cwd)
            .output();

        let diff_stat = match diff_output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                parse_diff_stat(&stdout)
            }
            _ => DiffStat::default(),
        };

        // Build output spans
        let mut spans = Vec::new();
        let mut has_content = false;

        // Modified files count
        let total_modified = status_info.modified + diff_stat.files_changed;
        if total_modified > 0 {
            spans.push(StyledSpan::new(
                format!("{}", total_modified),
                Style::default().fg(Color::Yellow),
            ));
            has_content = true;
        }

        // Diff stats (insertions/deletions)
        if diff_stat.insertions > 0 || diff_stat.deletions > 0 {
            if has_content {
                spans.push(StyledSpan::plain(" • "));
            }
            
            if diff_stat.insertions > 0 {
                spans.push(StyledSpan::new(
                    format!("+{}", diff_stat.insertions),
                    Style::default().fg(Color::Green),
                ));
            }
            
            if diff_stat.deletions > 0 {
                if diff_stat.insertions > 0 {
                    spans.push(StyledSpan::plain(" "));
                }
                spans.push(StyledSpan::new(
                    format!("-{}", diff_stat.deletions),
                    Style::default().fg(Color::Red),
                ));
            }
            has_content = true;
        }

        // Staged indicator
        if status_info.staged > 0 {
            if has_content {
                spans.push(StyledSpan::plain(" "));
            }
            spans.push(StyledSpan::new(
                format!("●{}", status_info.staged),
                Style::default().fg(Color::Green),
            ));
            has_content = true;
        }

        // Untracked indicator
        if status_info.untracked > 0 {
            if has_content {
                spans.push(StyledSpan::plain(" "));
            }
            spans.push(StyledSpan::new(
                format!("?{}", status_info.untracked),
                Style::default().fg(Color::BrightBlack),
            ));
            has_content = true;
        }

        if !has_content {
            return ComponentOutput::hidden();
        }

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "modified": status_info.modified,
                "staged": status_info.staged,
                "untracked": status_info.untracked,
                "insertions": diff_stat.insertions,
                "deletions": diff_stat.deletions,
            }),
        )
    }
}
```

**Step 4: Update builtins/mod.rs**

```rust
//! Built-in prompt components

pub mod exit_status;
pub mod git_branch;
pub mod git_status;
pub mod jobs;
pub mod pwd;

pub use exit_status::ExitStatusComponent;
pub use git_branch::GitBranchComponent;
pub use git_status::GitStatusComponent;
pub use jobs::JobsComponent;
pub use pwd::PwdComponent;
```

**Step 5: Update lib.rs exports**

```rust
pub use builtins::{
    ExitStatusComponent, GitBranchComponent, GitStatusComponent, JobsComponent, PwdComponent,
};
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement git_status component"
```

---

## Task 10: Implement Built-in Components - model

**Files:**
- Create: `crates/colony-ui-core/src/builtins/model.rs`
- Modify: `crates/colony-ui-core/src/builtins/mod.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for model component**

```rust
// crates/colony-ui-core/src/builtins/model.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentContext;
    use std::collections::HashMap;

    fn test_context(model: Option<&str>) -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: model.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_model_none_hidden() {
        let component = ModelComponent::new();
        let ctx = test_context(None);
        let output = component.render(&ctx);
        
        assert!(!output.visible);
    }

    #[test]
    fn test_model_visible() {
        let component = ModelComponent::new();
        let ctx = test_context(Some("gpt-4"));
        let output = component.render(&ctx);
        
        assert!(output.visible);
        assert!(output.spans[0].text.contains("gpt-4"));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement model component**

```rust
//! model component - shows current AI model when agent is active

use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use colony_tuple::Value;

/// model component
pub struct ModelComponent;

impl ModelComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ModelComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for ModelComponent {
    fn id(&self) -> &str {
        "model"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![vec![Value::Str("model_change".to_string()), Value::Wildcard]]
    }

    fn render(&self, ctx: &ComponentContext) -> ComponentOutput {
        let model = match &ctx.model {
            Some(m) => m,
            None => return ComponentOutput::hidden(),
        };

        let text = format!("🤖 {}", model);
        let spans = vec![StyledSpan::new(text, Style::default().fg(Color::Blue))];

        ComponentOutput::with_data(
            spans,
            serde_json::json!({
                "model": model,
            }),
        )
    }
}
```

**Step 4: Update builtins/mod.rs**

```rust
//! Built-in prompt components

pub mod exit_status;
pub mod git_branch;
pub mod git_status;
pub mod jobs;
pub mod model;
pub mod pwd;

pub use exit_status::ExitStatusComponent;
pub use git_branch::GitBranchComponent;
pub use git_status::GitStatusComponent;
pub use jobs::JobsComponent;
pub use model::ModelComponent;
pub use pwd::PwdComponent;
```

**Step 5: Update lib.rs exports**

```rust
pub use builtins::{
    ExitStatusComponent, GitBranchComponent, GitStatusComponent, JobsComponent, ModelComponent,
    PwdComponent,
};
```

**Step 6: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement model component"
```

---

## Task 11: Implement SubscriptionManager

**Files:**
- Create: `crates/colony-ui-core/src/subscription.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for SubscriptionManager**

```rust
// crates/colony-ui-core/src/subscription.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ComponentContext, ComponentOutput, PwdComponent, PromptComponent};
    use std::collections::HashMap;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/home/test"),
            env: {
                let mut env = HashMap::new();
                env.insert("HOME".to_string(), "/home/test".to_string());
                env
            },
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        }
    }

    #[test]
    fn test_register_component() {
        let mut manager = SubscriptionManager::new();
        let pwd = PwdComponent::new();
        
        manager.register(&pwd);
        
        assert!(manager.subscriptions.contains_key("dir_change"));
    }

    #[test]
    fn test_cache_invalidation() {
        let mut manager = SubscriptionManager::new();
        let pwd = PwdComponent::new();
        
        manager.register(&pwd);
        
        // Pre-populate cache
        let ctx = test_context();
        let _ = manager.get_output(&pwd, &ctx);
        
        assert!(manager.cache.contains_key("pwd"));
        
        // Invalidate
        manager.on_event("dir_change");
        
        assert!(!manager.cache.contains_key("pwd"));
    }

    #[test]
    fn test_cache_hit() {
        let mut manager = SubscriptionManager::new();
        let pwd = PwdComponent::new();
        
        manager.register(&pwd);
        
        let ctx = test_context();
        
        // First call populates cache
        let out1 = manager.get_output(&pwd, &ctx);
        
        // Second call should hit cache (same result)
        let out2 = manager.get_output(&pwd, &ctx);
        
        assert_eq!(out1.spans.len(), out2.spans.len());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement SubscriptionManager**

```rust
//! Subscription manager for component cache invalidation

use crate::component::{ComponentOutput, PromptComponent};
use crate::context::ComponentContext;
use colony_tuple::Value;
use std::collections::HashMap;

/// Manages component subscriptions and caching
pub struct SubscriptionManager {
    /// Map from event type to component IDs that care about it
    pub subscriptions: HashMap<String, Vec<String>>,
    /// Cached component outputs (invalidated on relevant events)
    pub cache: HashMap<String, ComponentOutput>,
}

impl SubscriptionManager {
    /// Create a new subscription manager
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    /// Register a component's subscriptions
    pub fn register(&mut self, component: &dyn PromptComponent) {
        for pattern in component.subscriptions() {
            // Extract event type (first element of tuple pattern)
            if let Some(Value::Str(event_type)) = pattern.first() {
                self.subscriptions
                    .entry(event_type.clone())
                    .or_default()
                    .push(component.id().to_string());
            }
        }
    }

    /// Called when tuple space emits an event
    pub fn on_event(&mut self, event_type: &str) {
        // Invalidate cache for affected components
        if let Some(component_ids) = self.subscriptions.get(event_type) {
            for id in component_ids {
                self.cache.remove(id);
            }
        }
    }

    /// Invalidate all cached outputs
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }

    /// Get component output (cached or fresh)
    pub fn get_output(
        &mut self,
        component: &dyn PromptComponent,
        ctx: &ComponentContext,
    ) -> ComponentOutput {
        // Check cache first
        if let Some(cached) = self.cache.get(component.id()) {
            return cached.clone();
        }

        // Render and cache
        let output = component.render(ctx);
        self.cache.insert(component.id().to_string(), output.clone());
        output
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 4: Update lib.rs**

```rust
//! Programmable prompt component system for Colony Shell

pub mod builtins;
pub mod component;
pub mod context;
pub mod render;
pub mod subscription;

pub use builtins::{
    ExitStatusComponent, GitBranchComponent, GitStatusComponent, JobsComponent, ModelComponent,
    PwdComponent,
};
pub use component::{
    Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern,
};
pub use context::ComponentContext;
pub use render::ansi::{render_outputs, render_prompt, strip_ansi};
pub use subscription::SubscriptionManager;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement SubscriptionManager"
```

---

## Task 12: Create ComponentRegistry

**Files:**
- Create: `crates/colony-ui-core/src/registry.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for ComponentRegistry**

```rust
// crates/colony-ui-core/src/registry.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_builtins() {
        let mut registry = ComponentRegistry::new();
        registry.register_builtins();
        
        assert!(registry.get("pwd").is_some());
        assert!(registry.get("git_branch").is_some());
        assert!(registry.get("exit_status").is_some());
        assert!(registry.get("jobs").is_some());
        assert!(registry.get("model").is_some());
        assert!(registry.get("git_status").is_some());
    }

    #[test]
    fn test_registry_get_unknown() {
        let registry = ComponentRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_all() {
        let mut registry = ComponentRegistry::new();
        registry.register_builtins();
        
        let all = registry.all();
        assert!(all.len() >= 6); // At least 6 builtins
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement ComponentRegistry**

```rust
//! Component registry for managing prompt components

use crate::builtins::{
    ExitStatusComponent, GitBranchComponent, GitStatusComponent, JobsComponent, ModelComponent,
    PwdComponent,
};
use crate::component::PromptComponent;
use std::collections::HashMap;

/// Registry of available prompt components
pub struct ComponentRegistry {
    components: HashMap<String, Box<dyn PromptComponent>>,
}

impl ComponentRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
        }
    }

    /// Register all built-in components
    pub fn register_builtins(&mut self) {
        self.register(Box::new(PwdComponent::new()));
        self.register(Box::new(GitBranchComponent::new()));
        self.register(Box::new(GitStatusComponent::new()));
        self.register(Box::new(JobsComponent::new()));
        self.register(Box::new(ExitStatusComponent::new()));
        self.register(Box::new(ModelComponent::new()));
    }

    /// Register a component
    pub fn register(&mut self, component: Box<dyn PromptComponent>) {
        self.components.insert(component.id().to_string(), component);
    }

    /// Get a component by ID
    pub fn get(&self, id: &str) -> Option<&dyn PromptComponent> {
        self.components.get(id).map(|c| c.as_ref())
    }

    /// Get all registered components
    pub fn all(&self) -> Vec<&dyn PromptComponent> {
        self.components.values().map(|c| c.as_ref()).collect()
    }

    /// Check if a component is registered
    pub fn contains(&self, id: &str) -> bool {
        self.components.contains_key(id)
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.register_builtins();
        registry
    }
}
```

**Step 4: Update lib.rs**

```rust
//! Programmable prompt component system for Colony Shell

pub mod builtins;
pub mod component;
pub mod context;
pub mod registry;
pub mod render;
pub mod subscription;

pub use builtins::{
    ExitStatusComponent, GitBranchComponent, GitStatusComponent, JobsComponent, ModelComponent,
    PwdComponent,
};
pub use component::{
    Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern,
};
pub use context::ComponentContext;
pub use registry::ComponentRegistry;
pub use render::ansi::{render_outputs, render_prompt, strip_ansi};
pub use subscription::SubscriptionManager;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement ComponentRegistry"
```

---

## Task 13: Implement PromptManager (High-level API)

**Files:**
- Create: `crates/colony-ui-core/src/manager.rs`
- Modify: `crates/colony-ui-core/src/lib.rs`

**Step 1: Write tests for PromptManager**

```rust
// crates/colony-ui-core/src/manager.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentContext;
    use std::collections::HashMap;

    fn test_context() -> ComponentContext {
        ComponentContext {
            cwd: std::path::PathBuf::from("/home/test/project"),
            env: {
                let mut env = HashMap::new();
                env.insert("HOME".to_string(), "/home/test".to_string());
                env
            },
            last_exit_code: 0,
            running_jobs: 0,
            stopped_jobs: 0,
            model: None,
        }
    }

    #[test]
    fn test_prompt_manager_default() {
        let manager = PromptManager::default();
        let ctx = test_context();
        let prompt = manager.render(&ctx, 80);
        
        // Should contain pwd at minimum
        assert!(prompt.contains("~/project") || prompt.contains("/home/test/project"));
        assert!(prompt.contains("❯"));
    }

    #[test]
    fn test_prompt_manager_custom_layout() {
        let manager = PromptManager::new(
            vec!["pwd".to_string()],
            vec!["exit_status".to_string()],
        );
        let ctx = test_context();
        let prompt = manager.render(&ctx, 80);
        
        assert!(prompt.contains("❯"));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p colony-ui-core`
Expected: FAIL

**Step 3: Implement PromptManager**

```rust
//! High-level prompt manager API

use crate::context::ComponentContext;
use crate::registry::ComponentRegistry;
use crate::render::ansi::render_prompt;
use crate::subscription::SubscriptionManager;

/// High-level prompt manager that coordinates components, caching, and rendering
pub struct PromptManager {
    registry: ComponentRegistry,
    subscription_manager: SubscriptionManager,
    left_components: Vec<String>,
    right_components: Vec<String>,
}

impl PromptManager {
    /// Create a new prompt manager with custom layout
    pub fn new(left: Vec<String>, right: Vec<String>) -> Self {
        let mut registry = ComponentRegistry::default();
        let mut subscription_manager = SubscriptionManager::new();

        // Register subscriptions for all components
        for component in registry.all() {
            subscription_manager.register(component);
        }

        Self {
            registry,
            subscription_manager,
            left_components: left,
            right_components: right,
        }
    }

    /// Handle a tuple space event (invalidates relevant component caches)
    pub fn on_event(&mut self, event_type: &str) {
        self.subscription_manager.on_event(event_type);
    }

    /// Render the prompt string
    pub fn render(&self, ctx: &ComponentContext, width: usize) -> String {
        // We need interior mutability for caching, but for now render fresh each time
        // In production, we'd use RefCell or similar
        let left_outputs: Vec<_> = self
            .left_components
            .iter()
            .filter_map(|id| self.registry.get(id))
            .map(|c| c.render(ctx))
            .collect();

        let right_outputs: Vec<_> = self
            .right_components
            .iter()
            .filter_map(|id| self.registry.get(id))
            .map(|c| c.render(ctx))
            .collect();

        render_prompt(&left_outputs, &right_outputs, width)
    }

    /// Get the component registry (for adding custom components)
    pub fn registry_mut(&mut self) -> &mut ComponentRegistry {
        &mut self.registry
    }
}

impl Default for PromptManager {
    fn default() -> Self {
        Self::new(
            vec![
                "pwd".to_string(),
                "git_branch".to_string(),
                "git_status".to_string(),
            ],
            vec![
                "model".to_string(),
                "jobs".to_string(),
                "exit_status".to_string(),
            ],
        )
    }
}
```

**Step 4: Update lib.rs**

```rust
//! Programmable prompt component system for Colony Shell

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
pub use component::{
    Color, ComponentOutput, PromptComponent, Style, StyledSpan, TuplePattern,
};
pub use context::ComponentContext;
pub use manager::PromptManager;
pub use registry::ComponentRegistry;
pub use render::ansi::{render_outputs, render_prompt, strip_ansi};
pub use subscription::SubscriptionManager;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p colony-ui-core`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/colony-ui-core/
git commit -m "feat(ui-core): implement PromptManager high-level API"
```

---

## Task 14: Integrate PromptManager into REPL

**Files:**
- Modify: `crates/colony-shell/Cargo.toml`
- Modify: `crates/colony-shell/src/repl.rs`

**Step 1: Add colony-ui-core dependency**

In `crates/colony-shell/Cargo.toml`, add to dependencies:
```toml
colony-ui-core = { workspace = true }
```

And add to workspace Cargo.toml:
```toml
colony-ui-core = { path = "crates/colony-ui-core" }
```

**Step 2: Verify build**

Run: `cargo check -p colony-shell`
Expected: Success

**Step 3: Modify repl.rs to use PromptManager**

Replace the static prompt logic in `run_repl` with PromptManager:

```rust
// Add import at top of crates/colony-shell/src/repl.rs
use colony_ui_core::{ComponentContext, PromptManager};

// In run_repl function, before the loop:
let prompt_manager = PromptManager::default();

// Replace the prompt variable logic in the loop:
// OLD:
// let prompt = if agent.is_some() {
//     "colony> "
// } else {
//     "shell> "
// };

// NEW:
let model_name = agent.as_ref().map(|a| a.model_name().to_string());
let ctx = ComponentContext::from_executor(&executor, model_name);
let terminal_width = terminal_size::terminal_size()
    .map(|(w, _)| w.0 as usize)
    .unwrap_or(80);
let prompt = prompt_manager.render(&ctx, terminal_width);
```

**Step 4: Add terminal_size dependency**

In `crates/colony-shell/Cargo.toml`:
```toml
terminal_size = "0.3"
```

**Step 5: Verify build**

Run: `cargo build -p colony-shell`
Expected: Success

**Step 6: Test manually**

Run: `cargo run -p colony-shell`
Expected: Prompt shows pwd, git info (if in repo), etc.

**Step 7: Commit**

```bash
git add crates/colony-shell/ Cargo.toml
git commit -m "feat(shell): integrate programmable prompt into REPL"
```

---

## Task 15: Wire Tuple Space Events to PromptManager

**Files:**
- Modify: `crates/colony-shell/src/repl.rs`

**Step 1: Add event handling after command execution**

After `execute_shell_command`, add code to notify the prompt manager of relevant events:

```rust
// After execute_shell_command returns successfully
match execute_shell_command(line, &mut executor).await {
    Ok(()) => {
        // Notify prompt manager of command completion
        // This invalidates exit_status cache
        prompt_manager.on_event("cmd_complete");
        
        // Display tuple activity (existing code)
        display_tuple_activity(&executor);
    }
    Err(e) => {
        eprintln!("Error: {}", e);
        prompt_manager.on_event("cmd_complete");
    }
}
```

**Step 2: Make prompt_manager mutable**

Change `let prompt_manager` to `let mut prompt_manager` in the function.

**Step 3: Verify build**

Run: `cargo build -p colony-shell`
Expected: Success

**Step 4: Test manually**

Run: `cargo run -p colony-shell`
- Run a failing command (e.g., `false`)
- Verify prompt shows exit status
- Run successful command
- Verify exit status disappears

**Step 5: Commit**

```bash
git add crates/colony-shell/
git commit -m "feat(shell): wire tuple space events to prompt manager"
```

---

## Task 16: Add OSC 2337 Prompt Protocol Message

**Files:**
- Modify: `crates/colony-protocol/src/lib.rs`

**Step 1: Add Prompt variant to ProtocolMessage**

```rust
// In ProtocolMessage enum
Prompt {
    left: Vec<PromptComponentData>,
    right: Vec<PromptComponentData>,
},

// Add new struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptComponentData {
    pub id: String,
    pub spans: Vec<PromptSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSpan {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub dim: bool,
    #[serde(default)]
    pub italic: bool,
}
```

**Step 2: Add emit_prompt function**

```rust
/// Emit prompt data for colony-terminal rendering
pub fn emit_prompt(left: Vec<PromptComponentData>, right: Vec<PromptComponentData>) {
    let msg = ProtocolMessage::Prompt { left, right };
    emit_message(&msg);
}
```

**Step 3: Add test**

```rust
#[test]
fn test_prompt_message() {
    let msg = ProtocolMessage::Prompt {
        left: vec![PromptComponentData {
            id: "pwd".to_string(),
            spans: vec![PromptSpan {
                text: "~/project".to_string(),
                fg: Some("cyan".to_string()),
                bg: None,
                bold: false,
                dim: false,
                italic: false,
            }],
            data: None,
        }],
        right: vec![],
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"Prompt\""));
    assert!(json.contains("pwd"));
}
```

**Step 4: Run tests**

Run: `cargo test -p colony-protocol`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/colony-protocol/
git commit -m "feat(protocol): add Prompt message type for OSC 2337"
```

---

## Task 17: Run Full Test Suite and Fix Any Issues

**Files:**
- Various (depends on test failures)

**Step 1: Run full workspace tests**

Run: `cargo test --workspace`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy --workspace`
Expected: No warnings (or only pre-existing ones)

**Step 3: Fix any issues found**

Address compiler warnings, clippy suggestions, and test failures.

**Step 4: Run final build**

Run: `cargo build --release -p colony-shell`
Expected: Success

**Step 5: Commit any fixes**

```bash
git add .
git commit -m "fix: address test failures and clippy warnings"
```

---

## Task 18: Update Documentation

**Files:**
- Modify: `docs/spec/14-builtins.md` (if exists)
- Modify: `README.md`
- Modify: `CHANGELOG.md`

**Step 1: Add prompt section to README**

Add a section describing the programmable prompt feature:

```markdown
## Programmable Prompt

Colony Shell features a Warp-style programmable prompt system with reactive components:

### Built-in Components
- `pwd` - Current directory (with ~ substitution)
- `git_branch` - Git branch name
- `git_status` - Modified files, insertions/deletions
- `jobs` - Background/stopped job count
- `exit_status` - Last exit code (when non-zero)
- `model` - Active AI model

### Default Layout
```
~/project  main 3 • +12 -5              🤖 gpt-4  ⚙ 2  ✘ 1
❯ 
```

Components react to events via tuple space subscriptions, only re-rendering when relevant state changes.
```

**Step 2: Update CHANGELOG**

```markdown
## [Unreleased]

### Added
- Programmable prompt component system (`colony-ui-core` crate)
- Built-in components: pwd, git_branch, git_status, jobs, exit_status, model
- ANSI renderer for terminal output
- OSC 2337 Prompt protocol message for colony-terminal
- SubscriptionManager for reactive cache invalidation
```

**Step 3: Commit documentation**

```bash
git add README.md CHANGELOG.md docs/
git commit -m "docs: add programmable prompt documentation"
```

---

## Summary

This plan implements the programmable prompt component system in 18 tasks:

1. **Tasks 1-4**: Core infrastructure (crate skeleton, types, context, renderer)
2. **Tasks 5-10**: Built-in components (pwd, exit_status, jobs, git_branch, git_status, model)
3. **Tasks 11-13**: Management layer (SubscriptionManager, ComponentRegistry, PromptManager)
4. **Tasks 14-15**: REPL integration
5. **Task 16**: Protocol extension
6. **Tasks 17-18**: Testing and documentation

Each task follows TDD (test first, then implement) and includes small, atomic commits.
