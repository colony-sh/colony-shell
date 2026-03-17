//! Parse bash/zsh config files into structured items.
//!
//! The importer reads shell configuration files and classifies each line
//! into one of several categories: alias, export, PATH entry, function,
//! tool initialization, or skipped (with reason). This structured data
//! is then used by the converter to generate a Colony profile.

use std::path::{Path, PathBuf};

/// A single imported item from a shell config file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportedItem {
    /// Shell alias: `alias ll='ls -la'`
    Alias { name: String, value: String },
    /// Environment export: `export EDITOR=vim`
    Export { name: String, value: String },
    /// PATH directory entry, extracted from `export PATH="...:$PATH"`
    PathEntry(String),
    /// Shell function definition
    Function { name: String, body: String },
    /// Tool initialization command: `eval "$(starship init zsh)"`
    ToolInit { tool: String, command: String },
    /// Line that was skipped with a reason
    Skipped { original: String, reason: String },
}

/// Result of parsing a single shell config file.
#[derive(Debug)]
pub struct ImportResult {
    /// Path to the source file that was parsed.
    pub source_file: PathBuf,
    /// Structured items extracted from the file.
    pub items: Vec<ImportedItem>,
    /// Total number of lines in the source file.
    pub line_count: usize,
}

impl ImportResult {
    /// Count items by category for display purposes.
    pub fn item_counts(&self) -> ItemCounts {
        let mut counts = ItemCounts::default();
        for item in &self.items {
            match item {
                ImportedItem::Alias { .. } => counts.aliases += 1,
                ImportedItem::Export { .. } => counts.exports += 1,
                ImportedItem::PathEntry(_) => counts.path_entries += 1,
                ImportedItem::Function { .. } => counts.functions += 1,
                ImportedItem::ToolInit { .. } => counts.tool_inits += 1,
                ImportedItem::Skipped { .. } => counts.skipped += 1,
            }
        }
        counts
    }
}

/// Counts of items by category.
#[derive(Debug, Default)]
pub struct ItemCounts {
    pub aliases: usize,
    pub exports: usize,
    pub path_entries: usize,
    pub functions: usize,
    pub tool_inits: usize,
    pub skipped: usize,
}

impl ItemCounts {
    /// Total number of successfully imported items (excludes skipped).
    pub fn imported_total(&self) -> usize {
        self.aliases + self.exports + self.path_entries + self.functions + self.tool_inits
    }
}

/// Detect available shell config files in the user's home directory.
///
/// Returns paths that actually exist on disk, in priority order
/// (zsh files first, then bash files).
pub fn detect_shell_configs() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let candidates = [
        ".zshrc",
        ".zprofile",
        ".zshenv",
        ".bashrc",
        ".bash_profile",
        ".profile",
    ];
    candidates
        .iter()
        .map(|f| home.join(f))
        .filter(|p| p.exists())
        .collect()
}

/// Detect which shell the user primarily uses based on the SHELL env var.
pub fn detect_primary_shell() -> &'static str {
    let Ok(shell) = std::env::var("SHELL") else {
        return "unknown";
    };
    if shell.contains("zsh") {
        return "zsh";
    }
    if shell.contains("bash") {
        return "bash";
    }
    "unknown"
}

/// Parse a shell config file into structured items.
///
/// Reads the file at `path` and classifies each non-empty, non-comment
/// line into an `ImportedItem`. Handles:
/// - Aliases (`alias name='value'`)
/// - Exports (`export VAR=value`)
/// - PATH mutations (`export PATH="...:$PATH"`)
/// - Tool initializations (`eval "$(tool init ...)"`)
/// - Source commands for known tools (`source ~/.nvm/nvm.sh`)
/// - Zsh-specific constructs (skipped with reason)
pub fn parse_config(path: &Path) -> ImportResult {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let line_count = content.lines().count();
    let items = parse_lines(&content);

    ImportResult {
        source_file: path.to_path_buf(),
        items,
        line_count,
    }
}

/// Parse lines of shell config content into structured items.
///
/// This is the core parsing logic, separated from file I/O for testability.
pub fn parse_lines(content: &str) -> Vec<ImportedItem> {
    let mut items = Vec::new();
    let mut in_function = false;
    let mut func_name = String::new();
    let mut func_body = String::new();
    let mut brace_depth: i32 = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments (unless inside a function body)
        if !in_function && (trimmed.is_empty() || trimmed.starts_with('#')) {
            continue;
        }

        // Track function bodies
        if in_function {
            brace_depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
            brace_depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;

            func_body.push_str(line);
            func_body.push('\n');

            if brace_depth <= 0 {
                in_function = false;
                items.push(ImportedItem::Function {
                    name: func_name.clone(),
                    body: func_body.trim_end().to_string(),
                });
                func_name.clear();
                func_body.clear();
            }
            continue;
        }

        // Function definitions: `function name() {` or `name() {`
        if let Some(name) = detect_function_start(trimmed) {
            in_function = true;
            func_name = name;
            func_body.clear();
            // Count braces on the opening line
            brace_depth = trimmed.chars().filter(|&c| c == '{').count() as i32;
            brace_depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;
            func_body.push_str(line);
            func_body.push('\n');

            // Single-line function
            if brace_depth <= 0 {
                in_function = false;
                items.push(ImportedItem::Function {
                    name: func_name.clone(),
                    body: func_body.trim_end().to_string(),
                });
                func_name.clear();
                func_body.clear();
            }
            continue;
        }

        // Aliases: `alias name='value'` or `alias name="value"`
        if let Some(item) = parse_alias(trimmed) {
            items.push(item);
            continue;
        }

        // Exports: `export VAR=value`
        if let Some(parsed) = parse_export(trimmed) {
            items.extend(parsed);
            continue;
        }

        // Tool initializations: `eval "$(tool init ...)"`
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

        // Zsh-specific options
        if trimmed.starts_with("setopt ") || trimmed.starts_with("unsetopt ") {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Zsh-specific option -- Colony uses POSIX set options".to_string(),
            });
            continue;
        }

        // Zsh completion system
        if trimmed.starts_with("autoload ")
            || trimmed.starts_with("compinit")
            || trimmed.starts_with("compdef ")
        {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Zsh completion system -- Colony has built-in completions".to_string(),
            });
            continue;
        }

        // Zsh keybindings
        if trimmed.starts_with("bindkey ") {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Zsh keybinding -- Colony uses vim/emacs mode config".to_string(),
            });
            continue;
        }

        // Oh My Zsh config
        if trimmed.starts_with("plugins=(") || trimmed.starts_with("ZSH_THEME=") {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Oh My Zsh config -- Colony uses its own plugin/recipe system".to_string(),
            });
            continue;
        }

        // Prompt definitions
        if trimmed.starts_with("PS1=") || trimmed.starts_with("PROMPT=") {
            items.push(ImportedItem::Skipped {
                original: trimmed.to_string(),
                reason: "Prompt definition -- Colony has its own prompt system".to_string(),
            });
            continue;
        }

        // Lines that don't match any pattern are silently ignored
        // (conditionals, loops, comments, blank lines, etc.)
    }

    items
}

/// Parse an alias line into an `ImportedItem`.
fn parse_alias(line: &str) -> Option<ImportedItem> {
    let rest = line.strip_prefix("alias ")?;
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();

    // Strip surrounding quotes from the value
    let value = strip_outer_quotes(value);

    Some(ImportedItem::Alias {
        name: name.to_string(),
        value: value.to_string(),
    })
}

/// Parse an export line into one or more `ImportedItem` values.
///
/// `export PATH="...:$PATH"` is split into individual `PathEntry` items.
/// Other exports become a single `Export` item.
fn parse_export(line: &str) -> Option<Vec<ImportedItem>> {
    let rest = line.strip_prefix("export ")?;
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    let value = strip_outer_quotes(value);

    if name == "PATH" {
        let entries: Vec<ImportedItem> = value
            .split(':')
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "$PATH" && *s != "${PATH}")
            .map(|s| ImportedItem::PathEntry(s.to_string()))
            .collect();
        Some(entries)
    } else {
        Some(vec![ImportedItem::Export {
            name: name.to_string(),
            value: value.to_string(),
        }])
    }
}

/// Detect the start of a function definition and return the function name.
fn detect_function_start(line: &str) -> Option<String> {
    // `function name() {` or `function name {`
    if let Some(rest) = line.strip_prefix("function ") {
        let name = rest
            .trim_start()
            .split(|c: char| c == '(' || c == '{' || c.is_whitespace())
            .next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    // `name() {`
    if let Some(paren_pos) = line.find("()") {
        let candidate = line[..paren_pos].trim();
        // Must be a single word (no spaces) and contain only valid identifier chars
        if !candidate.is_empty()
            && !candidate.contains(' ')
            && candidate
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            && line[paren_pos..].contains('{')
        {
            return Some(candidate.to_string());
        }
    }

    None
}

/// Detect which tool is being initialized from an `eval "$(...)"`  line.
fn detect_tool_init(line: &str) -> &str {
    if line.contains("starship init") {
        return "starship";
    }
    if line.contains("pyenv init") {
        return "pyenv";
    }
    if line.contains("rbenv init") {
        return "rbenv";
    }
    if line.contains("nodenv init") {
        return "nodenv";
    }
    if line.contains("direnv hook") {
        return "direnv";
    }
    if line.contains("brew shellenv") {
        return "homebrew";
    }
    if line.contains("fnm env") {
        return "fnm";
    }
    if line.contains("mise activate") {
        return "mise";
    }
    if line.contains("zoxide init") {
        return "zoxide";
    }
    if line.contains("fzf") {
        return "fzf";
    }
    if line.contains("atuin init") {
        return "atuin";
    }
    "unknown"
}

/// Detect known tools from `source` or `.` commands.
fn detect_source_tool(line: &str) -> &str {
    if line.contains("nvm.sh") {
        return "nvm";
    }
    if line.contains("cargo/env") {
        return "cargo";
    }
    if line.contains("asdf.sh") || line.contains("asdf/asdf") {
        return "asdf";
    }
    if line.contains("google-cloud-sdk") {
        return "gcloud";
    }
    if line.contains("rvm/scripts/rvm") {
        return "rvm";
    }
    if line.contains("sdkman") {
        return "sdkman";
    }
    ""
}

/// Strip matching outer quotes from a string value.
fn strip_outer_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse a single line of shell config.
    fn parse_line(line: &str) -> Vec<ImportedItem> {
        parse_lines(line)
    }

    // ── Alias tests ──────────────────────────────────────────

    #[test]
    fn parse_alias_single_quotes() {
        let items = parse_line("alias ll='ls -la'");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::Alias { name, value } if name == "ll" && value == "ls -la"
        ));
    }

    #[test]
    fn parse_alias_double_quotes() {
        let items = parse_line("alias gs=\"git status\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::Alias { name, value } if name == "gs" && value == "git status"
        ));
    }

    #[test]
    fn parse_alias_no_quotes() {
        let items = parse_line("alias c=clear");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::Alias { name, value } if name == "c" && value == "clear"
        ));
    }

    // ── Export tests ─────────────────────────────────────────

    #[test]
    fn parse_export_simple() {
        let items = parse_line("export EDITOR=vim");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::Export { name, value } if name == "EDITOR" && value == "vim"
        ));
    }

    #[test]
    fn parse_export_quoted() {
        let items = parse_line("export LANG=\"en_US.UTF-8\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::Export { name, value } if name == "LANG" && value == "en_US.UTF-8"
        ));
    }

    // ── PATH tests ───────────────────────────────────────────

    #[test]
    fn parse_path_splits_entries() {
        let items = parse_line("export PATH=\"$HOME/.cargo/bin:$HOME/.local/bin:$PATH\"");
        let path_entries: Vec<_> = items
            .iter()
            .filter(|i| matches!(i, ImportedItem::PathEntry(_)))
            .collect();
        assert_eq!(path_entries.len(), 2);
        assert!(matches!(&path_entries[0], ImportedItem::PathEntry(p) if p == "$HOME/.cargo/bin"));
        assert!(matches!(&path_entries[1], ImportedItem::PathEntry(p) if p == "$HOME/.local/bin"));
    }

    #[test]
    fn parse_path_filters_dollar_path() {
        let items = parse_line("export PATH=\"/usr/local/bin:$PATH\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::PathEntry(p) if p == "/usr/local/bin"));
    }

    #[test]
    fn parse_path_filters_braced_path() {
        let items = parse_line("export PATH=\"/opt/bin:${PATH}\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::PathEntry(p) if p == "/opt/bin"));
    }

    // ── Tool init tests ──────────────────────────────────────

    #[test]
    fn parse_tool_init_starship() {
        let items = parse_line("eval \"$(starship init zsh)\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "starship"
        ));
    }

    #[test]
    fn parse_tool_init_pyenv() {
        let items = parse_line("eval \"$(pyenv init -)\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "pyenv"
        ));
    }

    #[test]
    fn parse_tool_init_direnv() {
        let items = parse_line("eval \"$(direnv hook zsh)\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "direnv"
        ));
    }

    #[test]
    fn parse_tool_init_homebrew() {
        let items = parse_line("eval \"$(brew shellenv)\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "homebrew"
        ));
    }

    #[test]
    fn parse_tool_init_mise() {
        let items = parse_line("eval \"$(mise activate zsh)\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "mise"
        ));
    }

    #[test]
    fn parse_tool_init_unknown() {
        let items = parse_line("eval \"$(somenewtool init)\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "unknown"
        ));
    }

    // ── Source tool tests ────────────────────────────────────

    #[test]
    fn parse_source_nvm() {
        let items = parse_line("source ~/.nvm/nvm.sh");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "nvm"
        ));
    }

    #[test]
    fn parse_source_cargo() {
        let items = parse_line(". \"$HOME/.cargo/env\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "cargo"
        ));
    }

    #[test]
    fn parse_source_asdf() {
        let items = parse_line("source $HOME/.asdf/asdf.sh");
        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            ImportedItem::ToolInit { tool, .. } if tool == "asdf"
        ));
    }

    // ── Skipped item tests ───────────────────────────────────

    #[test]
    fn skip_setopt() {
        let items = parse_line("setopt AUTO_CD");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Skipped { reason, .. }
            if reason.contains("POSIX set options")));
    }

    #[test]
    fn skip_unsetopt() {
        let items = parse_line("unsetopt BEEP");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Skipped { .. }));
    }

    #[test]
    fn skip_autoload() {
        let items = parse_line("autoload -U compinit");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Skipped { reason, .. }
            if reason.contains("completion")));
    }

    #[test]
    fn skip_compinit() {
        let items = parse_line("compinit");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Skipped { .. }));
    }

    #[test]
    fn skip_bindkey() {
        let items = parse_line("bindkey '^R' history-search-backward");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Skipped { reason, .. }
            if reason.contains("keybinding")));
    }

    #[test]
    fn skip_oh_my_zsh_plugins() {
        let items = parse_line("plugins=(git docker node)");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Skipped { reason, .. }
            if reason.contains("Oh My Zsh")));
    }

    #[test]
    fn skip_zsh_theme() {
        let items = parse_line("ZSH_THEME=\"powerlevel10k\"");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Skipped { .. }));
    }

    #[test]
    fn skip_prompt_ps1() {
        let items = parse_line("PS1='\\u@\\h:\\w\\$ '");
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Skipped { reason, .. }
            if reason.contains("prompt")));
    }

    // ── Function tests ───────────────────────────────────────

    #[test]
    fn parse_function_keyword_style() {
        let input = "function mkcd() {\n    mkdir -p \"$1\" && cd \"$1\"\n}";
        let items = parse_lines(input);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Function { name, .. } if name == "mkcd"));
    }

    #[test]
    fn parse_function_posix_style() {
        let input = "greet() {\n    echo \"Hello, $1\"\n}";
        let items = parse_lines(input);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ImportedItem::Function { name, .. } if name == "greet"));
    }

    // ── Comment and blank line tests ─────────────────────────

    #[test]
    fn skip_comments() {
        let items = parse_line("# This is a comment");
        assert!(items.is_empty());
    }

    #[test]
    fn skip_blank_lines() {
        let items = parse_line("");
        assert!(items.is_empty());
    }

    #[test]
    fn skip_whitespace_only() {
        let items = parse_line("   ");
        assert!(items.is_empty());
    }

    // ── Integration / multi-line tests ───────────────────────

    #[test]
    fn parse_realistic_zshrc() {
        let content = r#"
# My zsh config
export EDITOR="nvim"
export VISUAL="nvim"
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

alias ll='ls -la'
alias gs='git status'
alias gp='git push'

eval "$(starship init zsh)"
eval "$(pyenv init -)"

setopt AUTO_CD
bindkey '^R' history-search-backward
plugins=(git docker node)

function mkcd() {
    mkdir -p "$1" && cd "$1"
}
"#;

        let items = parse_lines(content);

        let aliases: Vec<_> = items
            .iter()
            .filter(|i| matches!(i, ImportedItem::Alias { .. }))
            .collect();
        let exports: Vec<_> = items
            .iter()
            .filter(|i| matches!(i, ImportedItem::Export { .. }))
            .collect();
        let paths: Vec<_> = items
            .iter()
            .filter(|i| matches!(i, ImportedItem::PathEntry(_)))
            .collect();
        let tool_inits: Vec<_> = items
            .iter()
            .filter(|i| matches!(i, ImportedItem::ToolInit { .. }))
            .collect();
        let skipped: Vec<_> = items
            .iter()
            .filter(|i| matches!(i, ImportedItem::Skipped { .. }))
            .collect();
        let functions: Vec<_> = items
            .iter()
            .filter(|i| matches!(i, ImportedItem::Function { .. }))
            .collect();

        assert_eq!(aliases.len(), 3, "expected 3 aliases");
        assert_eq!(exports.len(), 2, "expected 2 exports (EDITOR, VISUAL)");
        assert_eq!(paths.len(), 2, "expected 2 PATH entries");
        assert_eq!(tool_inits.len(), 2, "expected 2 tool inits");
        assert_eq!(skipped.len(), 3, "expected 3 skipped items");
        assert_eq!(functions.len(), 1, "expected 1 function");
    }

    #[test]
    fn parse_config_from_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join(".zshrc");
        std::fs::write(
            &file,
            "alias ll='ls -la'\nexport EDITOR=vim\nsetopt AUTO_CD\n",
        )
        .unwrap();

        let result = parse_config(&file);
        assert_eq!(result.line_count, 3);
        assert_eq!(result.items.len(), 3);
        assert_eq!(result.source_file, file);
    }

    #[test]
    fn parse_config_missing_file() {
        let result = parse_config(Path::new("/nonexistent/file"));
        assert_eq!(result.line_count, 0);
        assert!(result.items.is_empty());
    }

    #[test]
    fn item_counts_correct() {
        let items = parse_lines(
            "alias ll='ls -la'\nexport EDITOR=vim\nexport PATH=\"/usr/bin:$PATH\"\nsetopt FOO\n",
        );
        let result = ImportResult {
            source_file: PathBuf::from("test"),
            items,
            line_count: 4,
        };
        let counts = result.item_counts();
        assert_eq!(counts.aliases, 1);
        assert_eq!(counts.exports, 1);
        assert_eq!(counts.path_entries, 1);
        assert_eq!(counts.skipped, 1);
        assert_eq!(counts.imported_total(), 3);
    }

    #[test]
    fn detect_primary_shell_reads_env() {
        // This test just verifies the function doesn't panic
        let shell = detect_primary_shell();
        assert!(matches!(shell, "zsh" | "bash" | "unknown"));
    }
}
