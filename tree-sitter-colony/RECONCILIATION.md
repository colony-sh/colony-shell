# Parser Reconciliation Layer

Colony Shell uses **three parsers** for different purposes:

## The Three Parsers

### 1. Tree-sitter (Editor Features)
**Location:** `tree-sitter-colony/`
**Implementation:** Pure Rust via `rust-sitter` (no C, no FFI)
**Purpose:** Real-time syntax highlighting, folding, navigation
**Characteristics:**
- Incremental parsing (< 1ms for typical edits)
- Error recovery (works with syntax errors)
- Produces CST (Concrete Syntax Tree)
- Pure Rust code generation (zero overhead)
- Used by: editors, LSP, syntax highlighters

**When it runs:** Every keystroke during editing

### 2. Pest PEG Parser (Validation)
**Location:** `crates/colony-parser/` (F021 branch)
**Purpose:** Strict POSIX validation before execution
**Characteristics:**
- Declarative grammar (`colony.pest`)
- PEG parsing (unambiguous)
- Better error messages than hand-written
- Produces validated AST

**When it runs:** Before executing a command

### 3. Hand-written Parser (Legacy/Fallback)
**Location:** `crates/colony-parser/` (main branch, F001/F002)
**Purpose:** Original implementation, now fallback
**Characteristics:**
- Recursive descent
- Full control over error messages
- Direct AST construction
- Complete POSIX support

**Status:** Being replaced by Pest (F021), but still functional

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    User Input / File                         │
└───────────────────────┬──────────────────────────────────────┘
                        │
        ┌───────────────┼───────────────────┐
        │               │                   │
        ▼               ▼                   ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ Tree-sitter  │  │  Pest PEG    │  │ Hand-written │
│   (fast)     │  │  (precise)   │  │  (fallback)  │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                  │
       │ CST             │ AST              │ AST
       ▼                 ▼                  ▼
┌──────────────────────────────────────────────────┐
│          Reconciliation Layer                    │
│                                                  │
│  • Tree-sitter CST → Highlighting               │
│  • Pest AST → Execution                         │
│  • Hand-written AST → Fallback                  │
└──────────────────────────────────────────────────┘
```

## Use Cases

### During Editing (Tree-sitter only)
```
User types: "echo hell"
  → Tree-sitter parses incrementally
  → Syntax highlighting updates instantly
  → Structure view updates
  → NO validation yet
```

### On Save / Pre-execution (Pest validation)
```
User runs: "echo hello | grep h"
  → Pest parser validates syntax
  → Produces AST for execution
  → If Pest fails → fallback to hand-written parser
  → If both fail → show error
```

### Execution Flow
```
1. User submits command
2. Check Tree-sitter parse (quick sanity check)
3. Parse with Pest (strict validation)
4. If Pest fails, try hand-written parser
5. Execute AST with colony-exec
```

## Reconciliation Strategy

### Phase 1: Editor Features Only
- Tree-sitter active for syntax highlighting
- Pest/hand-written used for execution
- No reconciliation needed (separate concerns)

### Phase 2: Error Hints
- Tree-sitter identifies error regions
- Pest validates those regions more strictly
- Show combined error messages:
  - Tree-sitter: "incomplete command"
  - Pest: "expected 'then' after 'if' condition"

### Phase 3: Incremental Validation (Future)
- Tree-sitter tracks which nodes changed
- Only re-validate changed subtrees with Pest
- Cache validated regions
- Reduces latency for large scripts

## Data Flow

### Tree-sitter CST → Highlighting
```rust
let mut parser = tree_sitter::Parser::new();
parser.set_language(tree_sitter_colony::language())?;

let tree = parser.parse(source, None)?;
let highlights = apply_highlights_query(&tree);
// Send to terminal for rendering
```

### Pest → Execution AST
```rust
let pairs = ColonyParser::parse(Rule::program, source)?;
let ast = build_ast(pairs)?;

let mut executor = Executor::new(context);
executor.execute(&ast).await?;
```

### Error Reconciliation
```rust
// Try Pest first (strict)
match pest_parse(source) {
    Ok(ast) => execute(ast),
    Err(pest_err) => {
        // Try hand-written (lenient)
        match handwritten_parse(source) {
            Ok(ast) => execute(ast),
            Err(hw_err) => {
                // Both failed - combine errors
                show_combined_error(pest_err, hw_err, tree_sitter_cst)
            }
        }
    }
}
```

## Implementation Status

- ✓ Tree-sitter grammar defined
- ✓ Rust bindings created
- ✓ Highlighting queries written
- ⏳ Pest parser (F021 in progress on feature/pest-parser branch)
- ✓ Hand-written parser (F001/F002 complete)
- ⏳ Reconciliation layer (this document is design spec)

## Integration Plan

### Step 1: Tree-sitter Generation
```bash
cd tree-sitter-colony
npm install
npx tree-sitter generate
npx tree-sitter test
```

### Step 2: Add to Workspace
```toml
# Cargo.toml
[workspace]
members = [
    # ... existing crates ...
    "tree-sitter-colony/bindings/rust",
]
```

### Step 3: Create Reconciliation Crate
```
crates/colony-reconcile/
  ├── Cargo.toml
  ├── src/
  │   ├── lib.rs         # Public API
  │   ├── tree_sitter.rs # CST utilities
  │   ├── validation.rs  # Multi-parser validation
  │   └── errors.rs      # Combined error reporting
```

### Step 4: Integration Points
- **REPL:** Tree-sitter for prompt, Pest for execution
- **File execution:** Pest primary, hand-written fallback
- **LSP:** Tree-sitter CST for all editor features
- **Error messages:** Combine insights from all three

## Benefits

### For Users
- Instant syntax feedback (Tree-sitter)
- Accurate error messages (Pest + hand-written)
- Always works (three-layer fallback)

### For Developers
- Declarative grammar easy to maintain (Tree-sitter + Pest)
- Performance where it matters (Tree-sitter incremental)
- Correctness guaranteed (Pest validation)
- Backward compatibility (hand-written fallback)

### For Colony Features
- Grammar evolution easier (modify Tree-sitter + Pest)
- Editor support automatic (Tree-sitter queries)
- Custom extensions simpler (add to all three, but Tree-sitter most important for UX)

## Next Steps

1. Generate Tree-sitter parser: `npx tree-sitter generate`
2. Test corpus: Expand `test/corpus/*.txt`
3. Create `colony-reconcile` crate
4. Integrate with REPL for real-time highlighting
5. Coordinate with F021 Pest parser completion
