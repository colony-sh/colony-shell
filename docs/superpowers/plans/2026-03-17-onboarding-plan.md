# Colony Shell Onboarding — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a first-run wizard that imports existing shell config, configures the LLM provider, installs the Claude plugin, and offers an interactive tutorial.

**Architecture:** New `onboarding` module in `colony-shell` crate with 4 components: wizard (step-by-step prompts), converter (profile import), importer (parse bash/zsh files), tutorial (wraps X-Ray). Wizard runs on first launch when `~/.colony/` doesn't exist.

**Tech Stack:** Rust, crossterm for colored terminal output, existing colony-shell infrastructure

**Spec:** `/Users/mcfearsome/src/github.com/colony-sh/colony-shell/docs/superpowers/specs/2026-03-17-onboarding-design.md`

---

## File Structure

```
crates/colony-shell/src/onboarding/
├── mod.rs              # First-run detection + orchestrator
├── wizard.rs           # 5-step wizard with terminal prompts
├── converter.rs        # Profile converter (coordinate import + write output)
├── importer.rs         # Parse bash/zsh config files into structured data
└── tutorial.rs         # Tutorial script runner
```

**Modify:**
- `crates/colony-shell/src/repl.rs` — call onboarding check before REPL loop
- `crates/colony-shell/src/lib.rs` or `main.rs` — register `pub mod onboarding`
- `crates/colony-exec/src/builtins.rs` — add `colony onboard` subcommand

---

## Chunk 1: Importer (Parse bash/zsh configs)

### Task 1: Shell config importer

**Files:**
- Create: `crates/colony-shell/src/onboarding/importer.rs`

- [ ] **Step 1: Define imported item types**

```rust
//! Parse bash/zsh config files into structured items.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum ImportedItem {
    Alias { name: String, value: String },
    Export { name: String, value: String },
    PathEntry(String),
    Function { name: String, body: String },
    ToolInit { tool: String, command: String },
    Skipped { original: String, reason: String },
}

#[derive(Debug)]
pub struct ImportResult {
    pub source_file: PathBuf,
    pub items: Vec<ImportedItem>,
    pub line_count: usize,
}

/// Detect available shell configs
pub fn detect_shell_configs() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let candidates = [
        ".zshrc", ".zprofile", ".zshenv",
        ".bashrc", ".bash_profile", ".profile",
    ];
    candidates.iter()
        .map(|f| home.join(f))
        .filter(|p| p.exists())
        .collect()
}

/// Detect which shell the user primarily uses
pub fn detect_primary_shell() -> &'static str {
    if let Ok(shell) = std::env::var("SHELL") {
        if shell.contains("zsh") { return "zsh"; }
        if shell.contains("bash") { return "bash"; }
    }
    "unknown"
}

/// Parse a shell config file into structured items
pub fn parse_config(path: &Path) -> ImportResult {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let line_count = content.lines().count();
    let mut items = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Aliases
        if let Some(rest) = trimmed.strip_prefix("alias ") {
            if let Some((name, value)) = rest.split_once('=') {
                let value = value.trim_matches('\'').trim_matches('"');
                items.push(ImportedItem::Alias {
                    name: name.to_string(),
                    value: value.to_string(),
                });
                continue;
            }
        }

        // Exports
        if let Some(rest) = trimmed.strip_prefix("export ") {
            if let Some((name, value)) = rest.split_once('=') {
                let value = value.trim_matches('"').trim_matches('\'');
                if name == "PATH" {
                    // Extract individual PATH entries
                    for segment in value.split(':') {
                        let segment = segment.trim();
                        if !segment.is_empty() && segment != "$PATH" {
                            items.push(ImportedItem::PathEntry(segment.to_string()));
                        }
                    }
                } else {
                    items.push(ImportedItem::Export {
                        name: name.to_string(),
                        value: value.to_string(),
                    });
                }
                continue;
            }
        }

        // Tool initializations (eval "$(tool init)")
        if trimmed.starts_with("eval \"$(") || trimmed.starts_with("eval \"$( ") {
            let tool = detect_tool_init(trimmed);
            items.push(ImportedItem::ToolInit {
                tool: tool.to_string(),
                command: trimmed.to_string(),
            });
            continue;
        }

        // Source commands for known tools
        if trimmed.starts_with("source ") || trimmed.starts_with(". ") {
            let tool = detect_source_tool(trimmed);
            if !tool.is_empty() {
                items.push(ImportedItem::ToolInit {
                    tool: tool.to_string(),
                    command: trimmed.to_string(),
                });
                continue;
            }
        }

        // Zsh-specific (skip with reason)
        if trimmed.starts_with("setopt ") || trimmed.starts_with("unsetopt ") {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Zsh-specific option — Colony uses POSIX set options".to_string(),
            });
            continue;
        }

        if trimmed.starts_with("autoload ") || trimmed.starts_with("compinit") || trimmed.starts_with("compdef ") {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Zsh completion system — Colony has built-in completions".to_string(),
            });
            continue;
        }

        if trimmed.starts_with("bindkey ") {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Zsh keybinding — Colony uses vim/emacs mode config".to_string(),
            });
            continue;
        }

        if trimmed.starts_with("plugins=(") || trimmed.starts_with("ZSH_THEME=") {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Oh My Zsh config — Colony uses its own plugin/recipe system".to_string(),
            });
            continue;
        }
    }

    ImportResult { source_file: path.to_path_buf(), items, line_count }
}

fn detect_tool_init(line: &str) -> &str {
    if line.contains("starship init") { return "starship"; }
    if line.contains("pyenv init") { return "pyenv"; }
    if line.contains("rbenv init") { return "rbenv"; }
    if line.contains("nodenv init") { return "nodenv"; }
    if line.contains("direnv hook") { return "direnv"; }
    if line.contains("brew shellenv") { return "homebrew"; }
    if line.contains("fnm env") { return "fnm"; }
    if line.contains("mise activate") { return "mise"; }
    if line.contains("zoxide init") { return "zoxide"; }
    if line.contains("fzf") { return "fzf"; }
    "unknown"
}

fn detect_source_tool(line: &str) -> &str {
    if line.contains("nvm.sh") { return "nvm"; }
    if line.contains("cargo/env") { return "cargo"; }
    if line.contains("asdf.sh") || line.contains("asdf/asdf") { return "asdf"; }
    if line.contains("google-cloud-sdk") { return "gcloud"; }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_alias() {
        let items = parse_line("alias ll='ls -la'");
        assert!(matches!(&items[0], ImportedItem::Alias { name, value } if name == "ll" && value == "ls -la"));
    }

    #[test]
    fn parse_export() {
        let items = parse_line("export EDITOR=vim");
        assert!(matches!(&items[0], ImportedItem::Export { name, value } if name == "EDITOR" && value == "vim"));
    }

    #[test]
    fn parse_path_splits() {
        let items = parse_line("export PATH=\"$HOME/.cargo/bin:$HOME/.local/bin:$PATH\"");
        let path_entries: Vec<_> = items.iter().filter(|i| matches!(i, ImportedItem::PathEntry(_))).collect();
        assert_eq!(path_entries.len(), 2);
    }

    #[test]
    fn parse_tool_init() {
        let items = parse_line("eval \"$(starship init zsh)\"");
        assert!(matches!(&items[0], ImportedItem::ToolInit { tool, .. } if tool == "starship"));
    }

    #[test]
    fn skip_zsh_specific() {
        let items = parse_line("setopt AUTO_CD");
        assert!(matches!(&items[0], ImportedItem::Skipped { .. }));
    }

    fn parse_line(line: &str) -> Vec<ImportedItem> {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), line).unwrap();
        parse_config(tmp.path()).items
    }
}
```

- [ ] **Step 2: Verify**

Run: `cargo check -p colony-shell`

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(onboarding): shell config importer — parse bash/zsh into structured items"
```

---

## Chunk 2: Converter + Wizard

### Task 2: Profile converter (write ~/.colony/profile)

**Files:**
- Create: `crates/colony-shell/src/onboarding/converter.rs`

- [ ] **Step 1: Implement converter**

Takes `Vec<ImportResult>` from the importer and writes `~/.colony/profile`:

```rust
//! Convert imported shell config items into a Colony profile.

use std::collections::LinkedHashSet;
use std::io::Write;
use std::path::Path;

use super::importer::{ImportResult, ImportedItem};

/// Generate the Colony profile content from import results.
pub fn generate_profile(results: &[ImportResult]) -> String {
    let mut output = String::new();
    let source_names: Vec<_> = results.iter()
        .map(|r| r.source_file.file_name().unwrap_or_default().to_string_lossy().to_string())
        .collect();

    output.push_str(&format!(
        "# Colony Profile — imported from {} on {}\n",
        source_names.join(", "),
        chrono::Local::now().format("%Y-%m-%d")
    ));
    output.push_str("# Lines marked [IMPORTED] were converted automatically.\n");
    output.push_str("# Lines marked [SKIPPED] need manual review.\n\n");

    // Collect all items from all sources
    let mut aliases = Vec::new();
    let mut exports = Vec::new();
    let mut path_entries = LinkedHashSet::new();
    let mut functions = Vec::new();
    let mut tool_inits = Vec::new();
    let mut skipped = Vec::new();

    for result in results {
        for item in &result.items {
            match item {
                ImportedItem::Alias { name, value } => aliases.push((name.clone(), value.clone())),
                ImportedItem::Export { name, value } => exports.push((name.clone(), value.clone())),
                ImportedItem::PathEntry(p) => { path_entries.insert(p.clone()); },
                ImportedItem::Function { name, body } => functions.push((name.clone(), body.clone())),
                ImportedItem::ToolInit { tool, command } => tool_inits.push((tool.clone(), command.clone())),
                ImportedItem::Skipped { original, reason } => skipped.push((original.clone(), reason.clone())),
            }
        }
    }

    // Aliases
    if !aliases.is_empty() {
        output.push_str("# ── Aliases ──────────────────────────────────\n");
        for (name, value) in &aliases {
            output.push_str(&format!("alias {}='{}'\t\t# [IMPORTED]\n", name, value));
        }
        output.push('\n');
    }

    // Exports
    if !exports.is_empty() {
        output.push_str("# ── Environment ──────────────────────────────\n");
        for (name, value) in &exports {
            output.push_str(&format!("export {}=\"{}\"\t\t# [IMPORTED]\n", name, value));
        }
        output.push('\n');
    }

    // PATH
    if !path_entries.is_empty() {
        output.push_str("# ── PATH ─────────────────────────────────────\n");
        output.push_str("# Merged from multiple sources, duplicates removed\n");
        let entries: Vec<_> = path_entries.into_iter().collect();
        output.push_str(&format!("export PATH=\"{}:$PATH\"\n\n", entries.join(":")));
    }

    // Functions
    if !functions.is_empty() {
        output.push_str("# ── Functions ────────────────────────────────\n");
        for (name, body) in &functions {
            output.push_str(&format!("{}\t\t# [IMPORTED]\n\n", body));
        }
    }

    // Tool inits
    if !tool_inits.is_empty() {
        output.push_str("# ── Tool Initialization ─────────────────────\n");
        for (tool, command) in &tool_inits {
            output.push_str(&format!("{}\t\t# [IMPORTED] {}\n", command, tool));
        }
        output.push('\n');
    }

    // Skipped
    if !skipped.is_empty() {
        output.push_str("# ── Skipped ──────────────────────────────────\n");
        for (original, reason) in &skipped {
            output.push_str(&format!("# {}\t\t# [SKIPPED] {}\n", original, reason));
        }
        output.push('\n');
    }

    output
}

/// Write the profile to ~/.colony/profile
pub fn write_profile(content: &str) -> std::io::Result<()> {
    let colony_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".colony");
    std::fs::create_dir_all(&colony_dir)?;
    let profile_path = colony_dir.join("profile");
    let mut file = std::fs::File::create(&profile_path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Summary stats for display
pub struct ConversionSummary {
    pub aliases: usize,
    pub exports: usize,
    pub path_entries: usize,
    pub functions: usize,
    pub tool_inits: usize,
    pub skipped: usize,
    pub total_lines: usize,
}

pub fn summarize(results: &[ImportResult]) -> ConversionSummary {
    let mut s = ConversionSummary {
        aliases: 0, exports: 0, path_entries: 0,
        functions: 0, tool_inits: 0, skipped: 0, total_lines: 0,
    };
    for r in results {
        s.total_lines += r.line_count;
        for item in &r.items {
            match item {
                ImportedItem::Alias { .. } => s.aliases += 1,
                ImportedItem::Export { .. } => s.exports += 1,
                ImportedItem::PathEntry(_) => s.path_entries += 1,
                ImportedItem::Function { .. } => s.functions += 1,
                ImportedItem::ToolInit { .. } => s.tool_inits += 1,
                ImportedItem::Skipped { .. } => s.skipped += 1,
            }
        }
    }
    s
}
```

- [ ] **Step 2: Commit**

```bash
git commit -m "feat(onboarding): profile converter — write ~/.colony/profile from imports"
```

### Task 3: First-run wizard

**Files:**
- Create: `crates/colony-shell/src/onboarding/wizard.rs`
- Create: `crates/colony-shell/src/onboarding/mod.rs`

- [ ] **Step 1: Implement wizard.rs**

5-step terminal-based wizard using simple stdin prompts with colored output via crossterm. Each step:
1. Print prompt with options
2. Read user selection (arrow keys or number)
3. Execute the action
4. Move to next step

Steps:
1. Import shell config (call importer + converter)
2. Keybinding style (vim/emacs/hybrid)
3. LLM provider + API key (verify with a test call)
4. Claude Code plugin install (call colony plugin install logic)
5. Quick tour offer (launch tutorial or skip)

Write `~/.colony/config.toml` with preferences.

- [ ] **Step 2: Implement mod.rs**

```rust
pub mod converter;
pub mod importer;
pub mod tutorial;
pub mod wizard;

/// Check if this is a first run and start onboarding if so.
pub fn check_first_run() -> bool {
    let colony_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".colony");

    if colony_dir.join("config.toml").exists() {
        return false; // Already onboarded
    }

    true
}
```

- [ ] **Step 3: Wire into repl.rs**

At the top of the REPL, before the prompt loop:
```rust
if onboarding::check_first_run() {
    onboarding::wizard::run().await;
}
```

- [ ] **Step 4: Add `colony onboard` subcommand to builtins**

In the `builtin_colony` match, add:
```rust
"onboard" => { /* launch wizard or tutorial */ }
```

- [ ] **Step 5: Build and test**

Run: `cargo check -p colony-shell`

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(onboarding): first-run wizard with 5-step setup + colony onboard builtin"
```
