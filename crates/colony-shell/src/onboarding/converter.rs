//! Convert imported shell config items into a Colony profile.
//!
//! Takes structured `ImportResult` data from the importer and generates
//! a `~/.colony/profile` file that Colony sources on startup. Items are
//! grouped by category with clear annotations marking imported vs skipped lines.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::importer::{ImportResult, ImportedItem};

/// Generate the Colony profile content from import results.
///
/// Produces a well-formatted shell script with sections for aliases,
/// environment variables, PATH, functions, tool inits, and skipped items.
/// Duplicate PATH entries are removed while preserving order.
pub fn generate_profile(results: &[ImportResult]) -> String {
    let mut output = String::new();
    let source_names: Vec<String> = results
        .iter()
        .filter_map(|r| {
            r.source_file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .collect();

    output.push_str(&format!(
        "# Colony Profile -- imported from {} on {}\n",
        source_names.join(", "),
        chrono::Local::now().format("%Y-%m-%d")
    ));
    output.push_str("# Lines marked [IMPORTED] were converted automatically.\n");
    output.push_str("# Lines marked [SKIPPED] need manual review.\n\n");

    // Collect all items from all sources, deduplicating as we go
    let mut aliases = Vec::new();
    let mut exports = Vec::new();
    let mut path_entries = Vec::new();
    let mut path_seen = BTreeSet::new();
    let mut functions = Vec::new();
    let mut tool_inits = Vec::new();
    let mut skipped = Vec::new();

    for result in results {
        for item in &result.items {
            match item {
                ImportedItem::Alias { name, value } => {
                    aliases.push((name.clone(), value.clone()));
                }
                ImportedItem::Export { name, value } => {
                    exports.push((name.clone(), value.clone()));
                }
                ImportedItem::PathEntry(p) => {
                    if path_seen.insert(p.clone()) {
                        path_entries.push(p.clone());
                    }
                }
                ImportedItem::Function { name, body } => {
                    functions.push((name.clone(), body.clone()));
                }
                ImportedItem::ToolInit { tool, command } => {
                    tool_inits.push((tool.clone(), command.clone()));
                }
                ImportedItem::Skipped { original, reason } => {
                    skipped.push((original.clone(), reason.clone()));
                }
            }
        }
    }

    // Aliases section
    if !aliases.is_empty() {
        output.push_str("# -- Aliases ------------------------------------------------\n");
        for (name, value) in &aliases {
            output.push_str(&format!("alias {name}='{value}'\t\t# [IMPORTED]\n"));
        }
        output.push('\n');
    }

    // Exports section
    if !exports.is_empty() {
        output.push_str("# -- Environment --------------------------------------------\n");
        for (name, value) in &exports {
            output.push_str(&format!("export {name}=\"{value}\"\t\t# [IMPORTED]\n"));
        }
        output.push('\n');
    }

    // PATH section
    if !path_entries.is_empty() {
        output.push_str("# -- PATH ---------------------------------------------------\n");
        output.push_str("# Merged from multiple sources, duplicates removed\n");
        output.push_str(&format!(
            "export PATH=\"{}:$PATH\"\n\n",
            path_entries.join(":")
        ));
    }

    // Functions section
    if !functions.is_empty() {
        output.push_str("# -- Functions ----------------------------------------------\n");
        for (_name, body) in &functions {
            output.push_str(&format!("{body}\t\t# [IMPORTED]\n\n"));
        }
    }

    // Tool inits section
    if !tool_inits.is_empty() {
        output.push_str("# -- Tool Initialization ------------------------------------\n");
        for (tool, command) in &tool_inits {
            output.push_str(&format!("{command}\t\t# [IMPORTED] {tool}\n"));
        }
        output.push('\n');
    }

    // Skipped section
    if !skipped.is_empty() {
        output.push_str("# -- Skipped ------------------------------------------------\n");
        for (original, reason) in &skipped {
            output.push_str(&format!("# {original}\t\t# [SKIPPED] {reason}\n"));
        }
        output.push('\n');
    }

    output
}

/// Write the profile content to a file at the given path.
///
/// Creates parent directories if they don't exist.
pub fn write_profile(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Default profile path: `~/.colony/profile`.
pub fn default_profile_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".colony").join("profile"))
}

/// Summary statistics for display after conversion.
#[derive(Debug, Default)]
pub struct ConversionSummary {
    pub aliases: usize,
    pub exports: usize,
    pub path_entries: usize,
    pub functions: usize,
    pub tool_inits: usize,
    pub skipped: usize,
    pub total_lines: usize,
}

impl ConversionSummary {
    /// Total number of successfully imported items.
    pub fn imported_total(&self) -> usize {
        self.aliases + self.exports + self.path_entries + self.functions + self.tool_inits
    }
}

impl std::fmt::Display for ConversionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} items imported ({} aliases, {} env vars, {} PATH entries, {} functions, {} tool inits), {} skipped",
            self.imported_total(),
            self.aliases,
            self.exports,
            self.path_entries,
            self.functions,
            self.tool_inits,
            self.skipped,
        )
    }
}

/// Build a conversion summary from import results.
pub fn summarize(results: &[ImportResult]) -> ConversionSummary {
    let mut s = ConversionSummary::default();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onboarding::importer::ImportedItem;

    fn make_result(items: Vec<ImportedItem>) -> ImportResult {
        ImportResult {
            source_file: PathBuf::from("/home/user/.zshrc"),
            items,
            line_count: 10,
        }
    }

    #[test]
    fn generate_profile_aliases() {
        let result = make_result(vec![ImportedItem::Alias {
            name: "ll".to_string(),
            value: "ls -la".to_string(),
        }]);
        let profile = generate_profile(&[result]);
        assert!(profile.contains("alias ll='ls -la'"));
        assert!(profile.contains("[IMPORTED]"));
    }

    #[test]
    fn generate_profile_exports() {
        let result = make_result(vec![ImportedItem::Export {
            name: "EDITOR".to_string(),
            value: "vim".to_string(),
        }]);
        let profile = generate_profile(&[result]);
        assert!(profile.contains("export EDITOR=\"vim\""));
    }

    #[test]
    fn generate_profile_path_dedup() {
        let result1 = make_result(vec![ImportedItem::PathEntry(
            "$HOME/.cargo/bin".to_string(),
        )]);
        let result2 = make_result(vec![
            ImportedItem::PathEntry("$HOME/.cargo/bin".to_string()),
            ImportedItem::PathEntry("$HOME/.local/bin".to_string()),
        ]);
        let profile = generate_profile(&[result1, result2]);
        // Should only appear once in the merged PATH
        let path_line = profile
            .lines()
            .find(|l| l.starts_with("export PATH="))
            .unwrap();
        assert_eq!(
            path_line.matches("$HOME/.cargo/bin").count(),
            1,
            "cargo/bin should appear once"
        );
        assert!(path_line.contains("$HOME/.local/bin"));
    }

    #[test]
    fn generate_profile_skipped_section() {
        let result = make_result(vec![ImportedItem::Skipped {
            original: "setopt AUTO_CD".to_string(),
            reason: "Zsh-specific".to_string(),
        }]);
        let profile = generate_profile(&[result]);
        assert!(profile.contains("# setopt AUTO_CD"));
        assert!(profile.contains("[SKIPPED]"));
    }

    #[test]
    fn generate_profile_tool_inits() {
        let result = make_result(vec![ImportedItem::ToolInit {
            tool: "starship".to_string(),
            command: "eval \"$(starship init bash)\"".to_string(),
        }]);
        let profile = generate_profile(&[result]);
        assert!(profile.contains("eval \"$(starship init bash)\""));
        assert!(profile.contains("starship"));
    }

    #[test]
    fn generate_profile_functions() {
        let result = make_result(vec![ImportedItem::Function {
            name: "mkcd".to_string(),
            body: "mkcd() {\n    mkdir -p \"$1\" && cd \"$1\"\n}".to_string(),
        }]);
        let profile = generate_profile(&[result]);
        assert!(profile.contains("mkcd()"));
        assert!(profile.contains("[IMPORTED]"));
    }

    #[test]
    fn write_profile_creates_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        let profile_path = dir.path().join("nested").join("dir").join("profile");
        write_profile(&profile_path, "# test profile\n").unwrap();
        assert!(profile_path.exists());
        let content = std::fs::read_to_string(&profile_path).unwrap();
        assert_eq!(content, "# test profile\n");
    }

    #[test]
    fn summarize_counts() {
        let result = make_result(vec![
            ImportedItem::Alias {
                name: "ll".to_string(),
                value: "ls -la".to_string(),
            },
            ImportedItem::Export {
                name: "EDITOR".to_string(),
                value: "vim".to_string(),
            },
            ImportedItem::PathEntry("/usr/bin".to_string()),
            ImportedItem::ToolInit {
                tool: "starship".to_string(),
                command: "eval ...".to_string(),
            },
            ImportedItem::Skipped {
                original: "setopt".to_string(),
                reason: "zsh".to_string(),
            },
        ]);
        let summary = summarize(&[result]);
        assert_eq!(summary.aliases, 1);
        assert_eq!(summary.exports, 1);
        assert_eq!(summary.path_entries, 1);
        assert_eq!(summary.tool_inits, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.imported_total(), 4);
        assert_eq!(summary.total_lines, 10);
    }

    #[test]
    fn summary_display() {
        let summary = ConversionSummary {
            aliases: 3,
            exports: 2,
            path_entries: 4,
            functions: 1,
            tool_inits: 2,
            skipped: 3,
            total_lines: 50,
        };
        let display = format!("{summary}");
        assert!(display.contains("12 items imported"));
        assert!(display.contains("3 skipped"));
    }
}
