//! Explanations for common external commands

use std::collections::HashMap;

pub struct CommandExplainer {
    commands: HashMap<&'static str, CommandInfo>,
}

pub struct CommandInfo {
    pub name: &'static str,
    pub brief: &'static str,
    pub common_flags: &'static [(&'static str, &'static str)],
}

impl Default for CommandExplainer {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandExplainer {
    pub fn new() -> Self {
        let mut commands = HashMap::new();

        commands.insert(
            "find",
            CommandInfo {
                name: "find",
                brief: "Search for files in directory hierarchy",
                common_flags: &[
                    ("-name", "match filename pattern"),
                    ("-type f", "find files only"),
                    ("-type d", "find directories only"),
                    ("-exec", "run command on each match"),
                    ("-mtime", "match by modification time"),
                ],
            },
        );

        commands.insert(
            "grep",
            CommandInfo {
                name: "grep",
                brief: "Search for patterns in text",
                common_flags: &[
                    ("-r", "recursive search"),
                    ("-i", "case insensitive"),
                    ("-v", "invert match (exclude)"),
                    ("-l", "list filenames only"),
                    ("-n", "show line numbers"),
                    ("-E", "extended regex"),
                ],
            },
        );

        commands.insert(
            "sed",
            CommandInfo {
                name: "sed",
                brief: "Stream editor for text transformation",
                common_flags: &[
                    ("-i", "edit files in-place"),
                    ("-e", "add editing command"),
                    ("-n", "suppress automatic printing"),
                    ("s/old/new/", "substitute pattern"),
                    ("s/old/new/g", "substitute all occurrences"),
                ],
            },
        );

        commands.insert(
            "awk",
            CommandInfo {
                name: "awk",
                brief: "Pattern scanning and text processing",
                common_flags: &[
                    ("-F", "set field separator"),
                    ("$1, $2", "access fields"),
                    ("NR", "line number"),
                    ("NF", "number of fields"),
                ],
            },
        );

        commands.insert(
            "xargs",
            CommandInfo {
                name: "xargs",
                brief: "Build and execute commands from stdin",
                common_flags: &[
                    ("-I{}", "replace {} with input"),
                    ("-n", "max args per command"),
                    ("-P", "parallel processes"),
                    ("-0", "null-separated input"),
                ],
            },
        );

        commands.insert(
            "sort",
            CommandInfo {
                name: "sort",
                brief: "Sort lines of text",
                common_flags: &[
                    ("-n", "numeric sort"),
                    ("-r", "reverse order"),
                    ("-u", "unique lines only"),
                    ("-k", "sort by specific field"),
                    ("-t", "field delimiter"),
                ],
            },
        );

        commands.insert(
            "uniq",
            CommandInfo {
                name: "uniq",
                brief: "Filter adjacent duplicate lines",
                common_flags: &[
                    ("-c", "count occurrences"),
                    ("-d", "only print duplicates"),
                    ("-u", "only print unique lines"),
                ],
            },
        );

        commands.insert(
            "cut",
            CommandInfo {
                name: "cut",
                brief: "Extract sections from lines",
                common_flags: &[
                    ("-d", "field delimiter"),
                    ("-f", "select fields"),
                    ("-c", "select characters"),
                ],
            },
        );

        commands.insert(
            "tee",
            CommandInfo {
                name: "tee",
                brief: "Read stdin, write to stdout and files",
                common_flags: &[("-a", "append to files")],
            },
        );

        commands.insert(
            "wc",
            CommandInfo {
                name: "wc",
                brief: "Count lines, words, characters",
                common_flags: &[
                    ("-l", "count lines"),
                    ("-w", "count words"),
                    ("-c", "count bytes"),
                    ("-m", "count characters"),
                ],
            },
        );

        commands.insert(
            "head",
            CommandInfo {
                name: "head",
                brief: "Output first part of files",
                common_flags: &[("-n", "number of lines"), ("-c", "number of bytes")],
            },
        );

        commands.insert(
            "tail",
            CommandInfo {
                name: "tail",
                brief: "Output last part of files",
                common_flags: &[
                    ("-n", "number of lines"),
                    ("-f", "follow (wait for new data)"),
                    ("-F", "follow and retry if file rotates"),
                ],
            },
        );

        commands.insert(
            "curl",
            CommandInfo {
                name: "curl",
                brief: "Transfer data from/to servers",
                common_flags: &[
                    ("-o", "output to file"),
                    ("-O", "save with remote filename"),
                    ("-L", "follow redirects"),
                    ("-s", "silent mode"),
                    ("-X", "HTTP method"),
                    ("-H", "add header"),
                    ("-d", "POST data"),
                ],
            },
        );

        commands.insert(
            "wget",
            CommandInfo {
                name: "wget",
                brief: "Download files from the web",
                common_flags: &[
                    ("-O", "output filename"),
                    ("-q", "quiet mode"),
                    ("-r", "recursive download"),
                    ("-c", "continue partial download"),
                ],
            },
        );

        commands.insert(
            "chmod",
            CommandInfo {
                name: "chmod",
                brief: "Change file permissions",
                common_flags: &[
                    ("+x", "add execute permission"),
                    ("-R", "recursive"),
                    ("755", "rwxr-xr-x (common for scripts)"),
                    ("644", "rw-r--r-- (common for files)"),
                ],
            },
        );

        commands.insert(
            "chown",
            CommandInfo {
                name: "chown",
                brief: "Change file owner and group",
                common_flags: &[("-R", "recursive"), ("user:group", "set both owner and group")],
            },
        );

        commands.insert(
            "tar",
            CommandInfo {
                name: "tar",
                brief: "Archive files",
                common_flags: &[
                    ("-c", "create archive"),
                    ("-x", "extract archive"),
                    ("-z", "gzip compression"),
                    ("-v", "verbose"),
                    ("-f", "archive filename"),
                ],
            },
        );

        commands.insert(
            "rsync",
            CommandInfo {
                name: "rsync",
                brief: "Fast file copying with delta sync",
                common_flags: &[
                    ("-a", "archive mode (preserve all)"),
                    ("-v", "verbose"),
                    ("-z", "compress during transfer"),
                    ("--delete", "delete extraneous files"),
                    ("-n", "dry run"),
                ],
            },
        );

        commands.insert(
            "jq",
            CommandInfo {
                name: "jq",
                brief: "JSON processor",
                common_flags: &[
                    (".", "identity (pretty print)"),
                    (".field", "extract field"),
                    (".[]", "iterate array"),
                    ("-r", "raw output (no quotes)"),
                    ("-c", "compact output"),
                ],
            },
        );

        commands.insert(
            "tr",
            CommandInfo {
                name: "tr",
                brief: "Translate or delete characters",
                common_flags: &[
                    ("-d", "delete characters"),
                    ("-s", "squeeze repeated chars"),
                    ("'[:lower:]' '[:upper:]'", "convert case"),
                ],
            },
        );

        Self { commands }
    }

    pub fn get(&self, command: &str) -> Option<&CommandInfo> {
        self.commands.get(command)
    }

    pub fn format_hint(&self, command: &str, args: &[String]) -> Option<String> {
        let info = self.get(command)?;

        let mut hint = info.brief.to_string();

        // Check for common flags in args
        for arg in args {
            for (flag, desc) in info.common_flags {
                if arg.starts_with(flag.split_whitespace().next().unwrap_or("")) {
                    hint.push_str(&format!("\n  {}: {}", flag, desc));
                    break;
                }
            }
        }

        Some(hint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_lookup() {
        let explainer = CommandExplainer::new();
        let info = explainer.get("grep").unwrap();
        assert_eq!(info.brief, "Search for patterns in text");
    }

    #[test]
    fn test_format_hint() {
        let explainer = CommandExplainer::new();
        let hint = explainer
            .format_hint("grep", &["-r".to_string(), "TODO".to_string()])
            .unwrap();
        assert!(hint.contains("recursive search"));
    }
}
