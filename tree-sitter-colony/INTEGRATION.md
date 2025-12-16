# Tree-sitter Integration Guide (rust-sitter)

How to integrate Tree-sitter Colony parser into editors and the Colony Shell REPL.

## Quick Start

### 1. Generate the Parser

```bash
cd tree-sitter-colony

# Generate grammar
npx tree-sitter generate
npx tree-sitter test
```

This generates:
- `src/grammar.json` - Compiled grammar
- `src/node-types.json` - Node type definitions

### 2. Generate Rust Code

```bash
cd bindings/rust

# Generate pure Rust parser (no C!)
rust-sitter generate --rust-package-info Cargo.toml
```

This generates:
- `src/parser.rs` - Pure Rust parser implementation

### 3. Build and Test

```bash
cargo build
cargo test
```

No C compilation, no FFI - just pure Rust!

### 3. Use in Colony Shell

```rust
use tree_sitter::Parser;
use tree_sitter_colony::language;

// Create parser
let mut parser = Parser::new();
parser.set_language(language())?;

// Parse code
let source = "echo hello | grep h";
let tree = parser.parse(source, None)?;

// Incremental re-parse
let new_source = "echo world | grep w";
let new_tree = parser.parse(new_source, Some(&tree))?;
```

## REPL Integration

### Real-time Highlighting

```rust
use tree_sitter::Parser;
use tree_sitter_colony::{language, HIGHLIGHTS_QUERY};

struct ReplHighlighter {
    parser: Parser,
    query: Query,
    old_tree: Option<Tree>,
}

impl ReplHighlighter {
    fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser.set_language(language())?;

        let query = Query::new(language(), HIGHLIGHTS_QUERY)?;

        Ok(Self {
            parser,
            query,
            old_tree: None,
        })
    }

    fn highlight(&mut self, source: &str) -> Result<Vec<Highlight>> {
        // Incremental parse
        let tree = self.parser.parse(source, self.old_tree.as_ref())?;

        // Query highlights
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&self.query, tree.root_node(), source.as_bytes());

        let highlights = extract_highlights(matches, source);

        // Save tree for next incremental parse
        self.old_tree = Some(tree);

        Ok(highlights)
    }
}
```

### Syntax Checking

```rust
fn check_syntax(source: &str) -> SyntaxStatus {
    let mut parser = Parser::new();
    parser.set_language(language())?;

    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    if has_error_nodes(&root) {
        SyntaxStatus::Invalid {
            errors: collect_error_positions(&root),
        }
    } else {
        SyntaxStatus::Valid
    }
}

fn has_error_nodes(node: &Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error_nodes(&child) {
            return true;
        }
    }

    false
}
```

## Editor Integration

### Neovim (via nvim-treesitter)

```lua
-- In ~/.config/nvim/init.lua or after/ftdetect/colony.lua

vim.filetype.add({
  extension = {
    colony = "colony",
  },
  pattern = {
    [".*%.sh"] = "colony",
  },
})

-- Register parser
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.colony = {
  install_info = {
    url = "~/src/colony-shell/tree-sitter-colony",
    files = {"src/parser.c"},
    branch = "main",
  },
  filetype = "colony",
}
```

### Helix

```toml
# In ~/.config/helix/languages.toml

[[language]]
name = "colony"
scope = "source.colony"
file-types = ["colony", "sh"]
roots = [".colonyrc"]
comment-token = "#"
indent = { tab-width = 2, unit = "  " }

[[grammar]]
name = "colony"
source = { path = "~/src/colony-shell/tree-sitter-colony" }
```

### Zed

```json
{
  "languages": {
    "Colony": {
      "grammar": "tree-sitter-colony",
      "path_suffixes": ["colony", "sh"]
    }
  }
}
```

## Advanced Queries

### Code Folding

```scheme
; queries/folds.scm

[
  (if_statement)
  (case_statement)
  (for_statement)
  (while_statement)
  (until_statement)
  (function_definition)
  (brace_group)
] @fold
```

### Symbol Extraction

```scheme
; queries/symbols.scm

(function_definition
  name: (function_name) @function.name) @function.definition

(variable_declaration
  name: (variable_name) @variable.name) @variable.definition
```

### Textobjects (for selection)

```scheme
; queries/textobjects.scm

(function_definition) @function.outer
(function_definition (brace_group) @function.inner)

(if_statement) @conditional.outer
(if_statement (simple_command) @conditional.inner)
```

## OSC 2337 Integration (Colony Terminal)

Send highlighting info to terminal via escape sequences:

```rust
fn send_highlights_to_terminal(highlights: &[Highlight]) -> Result<()> {
    // OSC 2337 ; Type=Highlight ; Data=<json> ST
    let json = serde_json::to_string(&highlights)?;
    print!("\x1b]2337;Type=Highlight;Data={}\x07", json);
    Ok(())
}

struct Highlight {
    start: usize,
    end: usize,
    kind: HighlightKind,
}

enum HighlightKind {
    Keyword,
    Command,
    String,
    Variable,
    Comment,
    Operator,
}
```

## Testing

### Parse Test

```bash
npx tree-sitter parse examples/test.sh
```

### Corpus Tests

```bash
npx tree-sitter test
```

### Rust Tests

```bash
cd bindings/rust
cargo test
```

## Performance Tips

1. **Reuse Parser:** Create once, reuse for all parses
2. **Keep Old Tree:** Pass to `parse()` for incremental updates
3. **Limit Query Scope:** Don't query entire tree if only checking one area
4. **Batch Highlights:** Collect all, then send to terminal once

## Troubleshooting

### Parser Generation Fails
- Check Node.js version (needs 16+)
- Ensure `tree-sitter-cli` installed: `npm install -g tree-sitter-cli`
- Check `grammar.js` for syntax errors

### rust-sitter Generation Fails
- Install rust-sitter: `cargo install rust-sitter-cli`
- Generate grammar first: `npx tree-sitter generate`
- Check rust-sitter.toml configuration

### Rust Build Fails
- Ensure parser.rs generated: `rust-sitter generate`
- No C compiler needed (pure Rust!)
- Check tree-sitter dependency version

### Highlights Not Showing
- Check query syntax: `npx tree-sitter query queries/highlights.scm`
- Verify terminal supports OSC 2337 (colony-terminal does)
- Test with `tree-sitter highlight examples/test.sh`

## Resources

- Tree-sitter docs: https://tree-sitter.github.io/tree-sitter/
- Grammar guide: https://tree-sitter.github.io/tree-sitter/creating-parsers
- Query syntax: https://tree-sitter.github.io/tree-sitter/syntax-highlighting
