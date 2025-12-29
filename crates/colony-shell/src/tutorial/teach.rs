//! First-occurrence teaching mode - tracks concepts and provides explanations
//!
//! When teach mode is enabled, the first time a shell construct is encountered,
//! a detailed explanation is provided. Subsequent occurrences are abbreviated.

use std::collections::HashSet;

/// Shell concepts that can be taught
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum Concept {
    // Parameter expansion variants
    ParameterDefault,       // ${var:-default}
    ParameterAssignDefault, // ${var:=default}
    ParameterError,         // ${var:?error}
    ParameterAlternate,     // ${var:+alternate}
    ParameterLength,        // ${#var}
    ParameterSubstring,     // ${var:offset:length}
    ParameterPattern,       // ${var#pattern}, ${var%pattern}

    // Quoting
    SingleQuote,     // 'literal'
    DoubleQuote,     // "interpolated"
    AnsiCQuote,      // $'escape\nsequences'
    BackslashEscape, // \char

    // Command substitution
    DollarParenSub, // $(command)
    BacktickSub,    // `command`

    // Arithmetic
    ArithmeticExpansion, // $((expr))
    ArithmeticCommand,   // ((expr))

    // Redirections
    OutputRedirect, // >
    AppendRedirect, // >>
    InputRedirect,  // <
    HereDoc,        // <<
    HereString,     // <<<
    FdDup,          // >&N, <&N
    FdClose,        // >&-, <&-

    // Pipes and lists
    Pipeline,   // |
    AndList,    // &&
    OrList,     // ||
    Background, // &
    Subshell,   // (commands)
    BraceGroup, // { commands; }

    // Control flow
    IfStatement,
    CaseStatement,
    ForLoop,
    WhileLoop,
    UntilLoop,
    SelectLoop,

    // Functions
    FunctionDefinition,
    FunctionCall,
    LocalVariable,
    ReturnStatement,

    // Globs
    GlobStar,     // *
    GlobQuestion, // ?
    GlobBracket,  // [...]
    ExtGlob,      // ?()|*()|+()|@()|!()
    GlobStar2,    // ** (recursive)

    // Arrays
    IndexedArray, // arr=(a b c), ${arr[0]}
    AssocArray,   // declare -A, ${arr[key]}
    ArrayAt,      // ${arr[@]}, ${arr[*]}
    ArrayLength,  // ${#arr[@]}

    // Special variables
    PositionalParams,  // $1, $2, etc.
    AllParams,         // $@, $*
    ParamCount,        // $#
    LastExitCode,      // $?
    CurrentPid,        // $$
    LastBackgroundPid, // $!
    ShellOptions,      // $-

    // Process substitution
    ProcessSubInput,  // <(command)
    ProcessSubOutput, // >(command)

    // Conditionals
    TestBuiltin, // [ expr ]
    TestBracket, // [[ expr ]]

    // Misc
    Alias,
    Trap,
    Coprocess,
}

impl Concept {
    /// Get the human-readable name of this concept
    pub fn name(&self) -> &'static str {
        match self {
            Self::ParameterDefault => "Parameter Expansion with Default",
            Self::ParameterAssignDefault => "Parameter Expansion with Assign Default",
            Self::ParameterError => "Parameter Expansion with Error",
            Self::ParameterAlternate => "Parameter Expansion with Alternate",
            Self::ParameterLength => "Parameter Length",
            Self::ParameterSubstring => "Parameter Substring",
            Self::ParameterPattern => "Parameter Pattern Matching",

            Self::SingleQuote => "Single Quoting",
            Self::DoubleQuote => "Double Quoting",
            Self::AnsiCQuote => "ANSI-C Quoting",
            Self::BackslashEscape => "Backslash Escaping",

            Self::DollarParenSub => "Command Substitution $(...)",
            Self::BacktickSub => "Command Substitution `...`",

            Self::ArithmeticExpansion => "Arithmetic Expansion",
            Self::ArithmeticCommand => "Arithmetic Command",

            Self::OutputRedirect => "Output Redirection",
            Self::AppendRedirect => "Append Redirection",
            Self::InputRedirect => "Input Redirection",
            Self::HereDoc => "Here Document",
            Self::HereString => "Here String",
            Self::FdDup => "File Descriptor Duplication",
            Self::FdClose => "File Descriptor Close",

            Self::Pipeline => "Pipeline",
            Self::AndList => "AND List (&&)",
            Self::OrList => "OR List (||)",
            Self::Background => "Background Execution",
            Self::Subshell => "Subshell",
            Self::BraceGroup => "Brace Group",

            Self::IfStatement => "If Statement",
            Self::CaseStatement => "Case Statement",
            Self::ForLoop => "For Loop",
            Self::WhileLoop => "While Loop",
            Self::UntilLoop => "Until Loop",
            Self::SelectLoop => "Select Loop",

            Self::FunctionDefinition => "Function Definition",
            Self::FunctionCall => "Function Call",
            Self::LocalVariable => "Local Variable",
            Self::ReturnStatement => "Return Statement",

            Self::GlobStar => "Glob Wildcard (*)",
            Self::GlobQuestion => "Glob Single Character (?)",
            Self::GlobBracket => "Glob Character Class ([...])",
            Self::ExtGlob => "Extended Glob Pattern",
            Self::GlobStar2 => "Recursive Glob (**)",

            Self::IndexedArray => "Indexed Array",
            Self::AssocArray => "Associative Array",
            Self::ArrayAt => "Array Expansion (@/*)",
            Self::ArrayLength => "Array Length",

            Self::PositionalParams => "Positional Parameters",
            Self::AllParams => "All Parameters ($@/$*)",
            Self::ParamCount => "Parameter Count ($#)",
            Self::LastExitCode => "Last Exit Code ($?)",
            Self::CurrentPid => "Current Process ID ($$)",
            Self::LastBackgroundPid => "Last Background PID ($!)",
            Self::ShellOptions => "Shell Options ($-)",

            Self::ProcessSubInput => "Process Substitution (Input)",
            Self::ProcessSubOutput => "Process Substitution (Output)",

            Self::TestBuiltin => "Test Builtin [ ]",
            Self::TestBracket => "Extended Test [[ ]]",

            Self::Alias => "Alias",
            Self::Trap => "Signal Trap",
            Self::Coprocess => "Coprocess",
        }
    }

    /// Get the detailed explanation for this concept
    pub fn explanation(&self) -> &'static str {
        match self {
            Self::ParameterDefault => {
                r#"${parameter:-word} expands to 'word' if parameter is unset or null.

  Syntax: ${var:-default}
  
  If $var is set and non-empty: use its value
  If $var is unset or empty: use "default"
  
  The variable itself is NOT modified.
  
  Example:
    name=${1:-guest}  # Use first arg, or "guest" if not provided"#
            }

            Self::ParameterAssignDefault => {
                r#"${parameter:=word} assigns 'word' to parameter if unset or null.

  Syntax: ${var:=default}
  
  If $var is set and non-empty: use its value
  If $var is unset or empty: assign "default" to var, then use it
  
  The variable IS modified (unlike :-)
  
  Example:
    : ${DEBUG:=0}  # Set DEBUG to 0 if not already set"#
            }

            Self::ParameterError => {
                r#"${parameter:?word} exits with error if parameter is unset or null.

  Syntax: ${var:?error message}
  
  If $var is set and non-empty: use its value
  If $var is unset or empty: print error message and exit
  
  Useful for required variables in scripts.
  
  Example:
    : ${REQUIRED_VAR:?must be set}"#
            }

            Self::ParameterAlternate => {
                r#"${parameter:+word} expands to 'word' only if parameter IS set.

  Syntax: ${var:+alternate}
  
  If $var is set and non-empty: use "alternate"
  If $var is unset or empty: expand to nothing
  
  The opposite of :- (use when set, not when unset)
  
  Example:
    echo ${DEBUG:+[DEBUG MODE]}"#
            }

            Self::Pipeline => {
                r#"A pipeline connects commands with | (pipe).

  Syntax: cmd1 | cmd2 | cmd3
  
  stdout of cmd1 → stdin of cmd2 → stdin of cmd3
  
  All commands run concurrently. Exit status is from the last command
  (unless pipefail is set).
  
  Example:
    cat file.txt | grep pattern | wc -l"#
            }

            Self::DollarParenSub => {
                r#"$(command) runs command and substitutes its output.

  Syntax: $(command)
  
  The command runs in a subshell. Its stdout replaces the $(...)
  Trailing newlines are removed.
  
  Can be nested: $(cat $(ls *.txt))
  
  Example:
    today=$(date +%Y-%m-%d)
    files=$(find . -name "*.rs" | wc -l)"#
            }

            Self::ForLoop => {
                r#"for loop iterates over a list of words.

  Syntax: for var in word1 word2 ...; do commands; done
  
  Assigns each word to $var and executes commands.
  Words undergo expansion (globs, variables, etc.)
  
  Example:
    for f in *.txt; do
      echo "Processing $f"
    done"#
            }

            Self::IfStatement => {
                r#"if statement executes commands conditionally.

  Syntax: if condition; then commands; [elif condition; then commands;] [else commands;] fi
  
  The condition is a command - exit 0 means true, non-zero means false.
  Commonly used with [ ] or [[ ]] for tests.
  
  Example:
    if [ -f "$file" ]; then
      echo "File exists"
    else
      echo "File not found"
    fi"#
            }

            Self::TestBuiltin => {
                r#"[ ] is the test builtin for conditional expressions.

  Common tests:
    -f file    File exists and is regular
    -d dir     Directory exists
    -e path    Path exists
    -z string  String is empty
    -n string  String is non-empty
    str = str  Strings are equal
    n -eq m    Numbers are equal
  
  Example:
    if [ -f "$file" ] && [ -r "$file" ]; then
      cat "$file"
    fi"#
            }

            Self::HereDoc => {
                r#"<<DELIM reads input until a line matching DELIM.

  Syntax: cmd <<DELIM
          content
          DELIM
  
  Content is fed to cmd's stdin.
  Variables are expanded unless delimiter is quoted (<<'EOF').
  <<- strips leading tabs.
  
  Example:
    cat <<EOF
    Hello, $USER
    Today is $(date)
    EOF"#
            }

            Self::SingleQuote => {
                r#"'...' preserves literal value of all characters.

  No expansion, no escaping (except '' for a single quote).
  
  Example:
    echo '$HOME'     # prints: $HOME
    echo 'line1
    line2'           # preserves newline"#
            }

            Self::DoubleQuote => {
                r#""..." allows $, `, \, and ! to be interpreted.

  Variables and command substitution work inside.
  Backslash only escapes $, `, ", \, and newline.
  
  Example:
    echo "Home is $HOME"      # expands $HOME
    echo "Files: $(ls)"       # command substitution
    echo "A \"quote\""        # escaped quote"#
            }

            // Default for concepts without detailed explanations
            _ => "See bash manual for details on this construct.",
        }
    }

    /// Get related concepts
    pub fn related(&self) -> Vec<Self> {
        match self {
            Self::ParameterDefault => vec![
                Self::ParameterAssignDefault,
                Self::ParameterError,
                Self::ParameterAlternate,
            ],
            Self::ParameterAssignDefault => vec![
                Self::ParameterDefault,
                Self::ParameterError,
                Self::ParameterAlternate,
            ],
            Self::DollarParenSub => vec![Self::BacktickSub, Self::Pipeline],
            Self::BacktickSub => vec![Self::DollarParenSub],
            Self::SingleQuote => vec![Self::DoubleQuote, Self::AnsiCQuote],
            Self::DoubleQuote => vec![Self::SingleQuote, Self::AnsiCQuote],
            Self::Pipeline => vec![Self::AndList, Self::OrList],
            Self::AndList => vec![Self::OrList, Self::Pipeline],
            Self::OrList => vec![Self::AndList, Self::Pipeline],
            Self::ForLoop => vec![Self::WhileLoop, Self::UntilLoop],
            Self::WhileLoop => vec![Self::ForLoop, Self::UntilLoop],
            Self::TestBuiltin => vec![Self::TestBracket],
            Self::TestBracket => vec![Self::TestBuiltin],
            Self::IndexedArray => vec![Self::AssocArray, Self::ArrayAt],
            Self::AssocArray => vec![Self::IndexedArray],
            _ => vec![],
        }
    }

    /// Get the concept ID as a string (for serialization)
    pub fn id(&self) -> &'static str {
        match self {
            Self::ParameterDefault => "param_default",
            Self::ParameterAssignDefault => "param_assign_default",
            Self::ParameterError => "param_error",
            Self::ParameterAlternate => "param_alternate",
            Self::ParameterLength => "param_length",
            Self::ParameterSubstring => "param_substring",
            Self::ParameterPattern => "param_pattern",
            Self::SingleQuote => "single_quote",
            Self::DoubleQuote => "double_quote",
            Self::AnsiCQuote => "ansi_c_quote",
            Self::BackslashEscape => "backslash_escape",
            Self::DollarParenSub => "dollar_paren_sub",
            Self::BacktickSub => "backtick_sub",
            Self::ArithmeticExpansion => "arith_expansion",
            Self::ArithmeticCommand => "arith_command",
            Self::OutputRedirect => "output_redirect",
            Self::AppendRedirect => "append_redirect",
            Self::InputRedirect => "input_redirect",
            Self::HereDoc => "heredoc",
            Self::HereString => "herestring",
            Self::FdDup => "fd_dup",
            Self::FdClose => "fd_close",
            Self::Pipeline => "pipeline",
            Self::AndList => "and_list",
            Self::OrList => "or_list",
            Self::Background => "background",
            Self::Subshell => "subshell",
            Self::BraceGroup => "brace_group",
            Self::IfStatement => "if_statement",
            Self::CaseStatement => "case_statement",
            Self::ForLoop => "for_loop",
            Self::WhileLoop => "while_loop",
            Self::UntilLoop => "until_loop",
            Self::SelectLoop => "select_loop",
            Self::FunctionDefinition => "function_def",
            Self::FunctionCall => "function_call",
            Self::LocalVariable => "local_var",
            Self::ReturnStatement => "return_stmt",
            Self::GlobStar => "glob_star",
            Self::GlobQuestion => "glob_question",
            Self::GlobBracket => "glob_bracket",
            Self::ExtGlob => "extglob",
            Self::GlobStar2 => "globstar",
            Self::IndexedArray => "indexed_array",
            Self::AssocArray => "assoc_array",
            Self::ArrayAt => "array_at",
            Self::ArrayLength => "array_length",
            Self::PositionalParams => "positional_params",
            Self::AllParams => "all_params",
            Self::ParamCount => "param_count",
            Self::LastExitCode => "last_exit_code",
            Self::CurrentPid => "current_pid",
            Self::LastBackgroundPid => "last_bg_pid",
            Self::ShellOptions => "shell_options",
            Self::ProcessSubInput => "proc_sub_in",
            Self::ProcessSubOutput => "proc_sub_out",
            Self::TestBuiltin => "test_builtin",
            Self::TestBracket => "test_bracket",
            Self::Alias => "alias",
            Self::Trap => "trap",
            Self::Coprocess => "coprocess",
        }
    }
}

/// Tracks which concepts have been explained in this session
#[derive(Debug, Default)]
pub struct ConceptTracker {
    seen: HashSet<Concept>,
}

impl ConceptTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a concept has been explained
    #[allow(dead_code)]
    pub fn has_seen(&self, concept: Concept) -> bool {
        self.seen.contains(&concept)
    }

    /// Mark a concept as explained, returns true if this is the first time
    pub fn mark_seen(&mut self, concept: Concept) -> bool {
        self.seen.insert(concept)
    }

    /// Reset all concept tracking
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.seen.clear();
    }

    /// Get all seen concepts
    #[allow(dead_code)]
    pub fn seen_concepts(&self) -> impl Iterator<Item = &Concept> {
        self.seen.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concept_tracking() {
        let mut tracker = ConceptTracker::new();

        // First time seeing concept
        assert!(!tracker.has_seen(Concept::Pipeline));
        assert!(tracker.mark_seen(Concept::Pipeline));
        assert!(tracker.has_seen(Concept::Pipeline));

        // Second time - already seen
        assert!(!tracker.mark_seen(Concept::Pipeline));

        // Reset
        tracker.reset();
        assert!(!tracker.has_seen(Concept::Pipeline));
    }

    #[test]
    fn test_concept_info() {
        let concept = Concept::ParameterDefault;
        assert_eq!(concept.name(), "Parameter Expansion with Default");
        assert!(!concept.explanation().is_empty());
        assert!(!concept.related().is_empty());
        assert_eq!(concept.id(), "param_default");
    }
}
