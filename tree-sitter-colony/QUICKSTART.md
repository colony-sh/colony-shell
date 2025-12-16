# Tree-sitter Colony - Quick Start

## Prerequisites

```bash
# Install Node.js (16+ required)
# macOS:
brew install node

# Install Tree-sitter CLI globally
npm install -g tree-sitter-cli

# Or use npx (no global install)
npx tree-sitter --version
```

## Generate Parser

```bash
cd tree-sitter-colony

# Install dependencies
npm install

# Generate parser C code
npx tree-sitter generate

# This creates:
#   src/parser.c         - Generated parser
#   src/node-types.json  - Type definitions
#   src/grammar.json     - Compiled grammar
```

## Test Parser

```bash
# Run corpus tests
npx tree-sitter test

# Parse example file
npx tree-sitter parse examples/test.sh

# Test queries
npx tree-sitter highlight examples/test.sh
```

## Build Rust Bindings

```bash
cd bindings/rust

# Build (compiles src/parser.c via build.rs)
cargo build

# Run tests
cargo test

# All tests should pass:
#   - test_can_load_grammar
#   - test_parse_simple_command
#   - test_parse_pipeline
#   - test_incremental_parse
```

## Add to Colony Shell Workspace

```bash
cd ../../..  # Back to colony-shell root

# Edit Cargo.toml
# Add to [workspace.members]:
#   "tree-sitter-colony/bindings/rust"
```

## Verify Integration

```bash
# From colony-shell root
cargo test -p tree-sitter-colony

# Should see: "test result: ok. 4 passed"
```

## Next Steps

1. **Create reconciliation crate:**
   ```bash
   mkdir -p crates/colony-reconcile
   # Implement highlight provider, validator coordinator
   ```

2. **Integrate with REPL:**
   ```rust
   // In colony-shell/src/repl.rs
   use tree_sitter_colony::language;

   let mut parser = Parser::new();
   parser.set_language(language())?;

   // On each keystroke
   let tree = parser.parse(buffer, old_tree)?;
   send_highlights(tree)?;
   ```

3. **Add editor configs:**
   - Neovim: `~/.config/nvim/after/queries/colony/`
   - Helix: `~/.config/helix/languages.toml`
   - Zed: Language server config

## Troubleshooting

### "tree-sitter: command not found"
```bash
npm install -g tree-sitter-cli
# Or use: npx tree-sitter
```

### "Error: No such file: src/parser.c"
```bash
# You need to generate first
npx tree-sitter generate
```

### Rust build fails: "parser.c not found"
```bash
# Generate parser before building Rust bindings
cd ../..  # to tree-sitter-colony root
npx tree-sitter generate
cd bindings/rust
cargo build
```

### Tests fail: "language version mismatch"
```bash
# Regenerate with current tree-sitter version
npx tree-sitter generate
cargo clean
cargo test
```

## Development Workflow

```bash
# Edit grammar
vim grammar.js

# Regenerate
npx tree-sitter generate

# Test
npx tree-sitter test

# Update Rust bindings
cd bindings/rust
cargo test

# If tests pass, commit
git add .
git commit -m "Update Tree-sitter grammar: <description>"
```

## Resources

- Grammar: `grammar.js` (JavaScript DSL)
- Highlights: `queries/highlights.scm` (query language)
- Tests: `test/corpus/*.txt` (test format)
- Bindings: `bindings/rust/` (Rust integration)
- Docs: `README.md`, `RECONCILIATION.md`, `INTEGRATION.md`
