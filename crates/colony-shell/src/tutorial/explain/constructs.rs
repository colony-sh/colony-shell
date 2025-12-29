//! Explanations for shell constructs (parameter expansion, pipes, etc.)

use std::collections::HashMap;

#[allow(dead_code)]
pub struct ConceptExplainer {
    concepts: HashMap<&'static str, Concept>,
}

#[allow(dead_code)]
pub struct Concept {
    pub id: &'static str,
    pub name: &'static str,
    pub explanation: &'static str,
    pub related: &'static [&'static str],
}

impl Default for ConceptExplainer {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl ConceptExplainer {
    pub fn new() -> Self {
        let mut concepts = HashMap::new();

        // Parameter expansion variants
        concepts.insert(
            "param_default",
            Concept {
                id: "param_default",
                name: "Parameter Expansion with Default",
                explanation: r#"${var:-word} expands to the value of $var if set and non-null, otherwise expands to word.

The colon is optional:
- ${var:-word}  checks if var is unset OR null (empty)
- ${var-word}   checks if var is unset only"#,
                related: &["param_assign_default", "param_error", "param_alternate"],
            },
        );

        concepts.insert(
            "param_assign_default",
            Concept {
                id: "param_assign_default",
                name: "Parameter Expansion with Assignment",
                explanation: r#"${var:=word} is like ${var:-word}, but also assigns word to var if var was unset/null.

After this expansion, $var will have a value."#,
                related: &["param_default"],
            },
        );

        concepts.insert(
            "param_error",
            Concept {
                id: "param_error",
                name: "Parameter Expansion with Error",
                explanation: r#"${var:?message} expands to $var if set and non-null. If unset/null, prints message to stderr and exits.

Useful for required variables:
  ${CONFIG_FILE:?Config file not specified}"#,
                related: &["param_default"],
            },
        );

        concepts.insert(
            "param_alternate",
            Concept {
                id: "param_alternate",
                name: "Parameter Expansion with Alternate",
                explanation: r#"${var:+word} expands to word if $var is set and non-null, otherwise expands to nothing.

Useful for conditional arguments:
  cmd ${VERBOSE:+-v}  # adds -v only if VERBOSE is set"#,
                related: &["param_default"],
            },
        );

        concepts.insert(
            "param_length",
            Concept {
                id: "param_length",
                name: "Parameter Length",
                explanation: r#"${#var} expands to the length of $var's value in characters.

${#array[@]} gives the number of elements in an array."#,
                related: &[],
            },
        );

        concepts.insert(
            "param_substring",
            Concept {
                id: "param_substring",
                name: "Parameter Substring",
                explanation: r#"${var:offset:length} extracts a substring.

- offset: starting position (0-based)
- length: optional, number of characters

Negative offset counts from end:
  ${var: -3}  # last 3 characters (note the space)"#,
                related: &[],
            },
        );

        // Command substitution
        concepts.insert(
            "command_sub",
            Concept {
                id: "command_sub",
                name: "Command Substitution",
                explanation: r#"$(command) executes command and substitutes its stdout.

The shell:
1. Creates a subshell
2. Runs the command
3. Captures stdout
4. Replaces $(...) with the output (trailing newlines stripped)

Backticks `command` are equivalent but harder to nest."#,
                related: &["process_sub"],
            },
        );

        // Process substitution
        concepts.insert(
            "process_sub",
            Concept {
                id: "process_sub",
                name: "Process Substitution",
                explanation: r#"<(command) and >(command) create temporary file descriptors.

<(cmd) provides cmd's output as a readable file:
  diff <(sort file1) <(sort file2)

>(cmd) provides a writable file that feeds into cmd:
  tee >(gzip > backup.gz) < input"#,
                related: &["command_sub"],
            },
        );

        // Pipes
        concepts.insert(
            "pipe",
            Concept {
                id: "pipe",
                name: "Pipeline",
                explanation: r#"cmd1 | cmd2 connects cmd1's stdout to cmd2's stdin.

Both commands run simultaneously. Data flows through the pipe as it's produced - no intermediate files needed.

The exit status is cmd2's status (or set by pipefail)."#,
                related: &["pipe_stderr"],
            },
        );

        concepts.insert(
            "pipe_stderr",
            Concept {
                id: "pipe_stderr",
                name: "Pipe with Stderr",
                explanation: r#"|& pipes both stdout AND stderr to the next command.

Equivalent to: cmd1 2>&1 | cmd2"#,
                related: &["pipe"],
            },
        );

        // Redirections
        concepts.insert(
            "redirect_out",
            Concept {
                id: "redirect_out",
                name: "Output Redirection",
                explanation: r#"> file redirects stdout to file (creates or truncates).
>> file redirects stdout to file (creates or appends).

With file descriptor:
  2> file   - redirect stderr
  &> file   - redirect both stdout and stderr"#,
                related: &["redirect_in", "redirect_dup"],
            },
        );

        concepts.insert(
            "redirect_in",
            Concept {
                id: "redirect_in",
                name: "Input Redirection",
                explanation: r#"< file redirects file to stdin.

<<DELIM is a here-document: inline multiline input.
<<<word is a here-string: single line input."#,
                related: &["redirect_out"],
            },
        );

        concepts.insert(
            "redirect_dup",
            Concept {
                id: "redirect_dup",
                name: "File Descriptor Duplication",
                explanation: r#"2>&1 makes fd 2 (stderr) point to wherever fd 1 (stdout) goes.

Order matters:
  cmd > file 2>&1   # both to file
  cmd 2>&1 > file   # only stdout to file, stderr to terminal

n>&- closes file descriptor n."#,
                related: &["redirect_out"],
            },
        );

        // Quoting
        concepts.insert(
            "quote_single",
            Concept {
                id: "quote_single",
                name: "Single Quotes",
                explanation: r#"'text' preserves everything literally. No expansion, no escapes.

To include a single quote: 'it'\''s' or 'it'"'"'s'"#,
                related: &["quote_double", "quote_escape"],
            },
        );

        concepts.insert(
            "quote_double",
            Concept {
                id: "quote_double",
                name: "Double Quotes",
                explanation: r#""text" allows:
- Variable expansion: "$var"
- Command substitution: "$(cmd)"
- Escape sequences: "line1\nline2" (in $"...")

Prevents:
- Word splitting
- Glob expansion"#,
                related: &["quote_single"],
            },
        );

        concepts.insert(
            "quote_escape",
            Concept {
                id: "quote_escape",
                name: "Backslash Escape",
                explanation: r#"\ escapes the next character.

Outside quotes: escapes special characters
Inside double quotes: only escapes $ ` " \ newline
Inside single quotes: no effect (literal \)"#,
                related: &["quote_single", "quote_double"],
            },
        );

        // Arrays
        concepts.insert(
            "array_indexed",
            Concept {
                id: "array_indexed",
                name: "Indexed Arrays",
                explanation: r#"arr=(a b c) creates an indexed array.

Access elements: ${arr[0]}, ${arr[1]}
All elements: ${arr[@]} or ${arr[*]}
Length: ${#arr[@]}

"${arr[@]}" preserves elements as separate words."#,
                related: &["array_assoc"],
            },
        );

        concepts.insert(
            "array_assoc",
            Concept {
                id: "array_assoc",
                name: "Associative Arrays",
                explanation: r#"declare -A map creates an associative array.

map[key]=value sets a value.
${map[key]} gets a value.
${!map[@]} lists all keys."#,
                related: &["array_indexed"],
            },
        );

        // Special variables
        concepts.insert(
            "special_args",
            Concept {
                id: "special_args",
                name: "Positional Parameters",
                explanation: r#"$1, $2, ... are positional parameters (script/function arguments).

$0 is the script name.
$# is the argument count.
$@ and $* are all arguments:
- "$@" expands to separate words: "$1" "$2" ...
- "$*" expands to single word: "$1 $2 ...""#,
                related: &["special_status"],
            },
        );

        concepts.insert(
            "special_status",
            Concept {
                id: "special_status",
                name: "Exit Status Variables",
                explanation: r#"$? is the exit status of the last command (0 = success).
$! is the PID of the last background process.
$$ is the current shell's PID.
$- shows current shell options."#,
                related: &["special_args"],
            },
        );

        Self { concepts }
    }

    pub fn get(&self, id: &str) -> Option<&Concept> {
        self.concepts.get(id)
    }

    pub fn explain_parameter_expansion(&self, modifier: &str) -> Option<&Concept> {
        match modifier {
            ":-" | "-" => self.get("param_default"),
            ":=" | "=" => self.get("param_assign_default"),
            ":?" | "?" => self.get("param_error"),
            ":+" | "+" => self.get("param_alternate"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concept_lookup() {
        let explainer = ConceptExplainer::new();
        let concept = explainer.get("param_default").unwrap();
        assert_eq!(concept.name, "Parameter Expansion with Default");
    }

    #[test]
    fn test_modifier_lookup() {
        let explainer = ConceptExplainer::new();
        let concept = explainer.explain_parameter_expansion(":-").unwrap();
        assert_eq!(concept.id, "param_default");
    }
}
