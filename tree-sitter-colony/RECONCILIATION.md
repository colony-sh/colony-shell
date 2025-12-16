# Parser Reconciliation Layer

Colony Shell uses **two parsers** for complementary purposes:

## The Two Parsers

### 1. Tree-sitter (Editor Features Only)
**Location:** `tree-sitter-colony/`
**Implementation:** Forked from tree-sitter-bash (proven, 30k line grammar)
**Purpose:** Real-time syntax highlighting, folding, navigation
**Characteristics:**
- Incremental parsing (< 1ms for typical edits)
- Error recovery (works with syntax errors)
- Produces CST (Concrete Syntax Tree)
- Forked from tree-sitter-bash (battle-tested)
- Includes scanner.c for context-sensitive parsing
- Used by: editors, LSP, syntax highlighters

**When it runs:** Every keystroke during editing

### 2. Pest PEG Parser (Execution Only)
**Location:** `crates/colony-parser/` (F021)
**Purpose:** Strict validation and AST generation for execution
**Characteristics:**
- Declarative grammar (`colony.pest`)
- Pure PEG (no lexer needed - breakthrough!)
- Unambiguous parsing
- Detailed error messages
- Produces execution AST
- 60 tests passing

**When it runs:** Before executing a command

### ~~3. Hand-written Parser~~ (DEPRECATED)
**Status:** **F001/F002 deprecated** - Pest replaces it entirely

**Reason for Deprecation:**
- Pest pure PEG works without F001 lexer
- Grammar structure provides parsing context
- Simpler two-parser architecture
- Hand-written served its bootstrap purpose

## Architecture

```
┌──────────────────────────────────────────────────┐
│              User Input / File                   │
└──────────────────────┬───────────────────────────┘
                       │
          ┌────────────┴────────────┐
          │                         │
          ▼                         ▼
  ┌──────────────┐          ┌──────────────┐
  │ Tree-sitter  │          │   Pest PEG   │
  │  (editor)    │          │  (execution) │
  │              │          │              │
  │ • Fast       │          │ • Precise    │
  │ • Tolerant   │          │ • Validated  │
  │ • Bash fork  │          │ • Pure PEG   │
  └──────┬───────┘          └──────┬───────┘
         │                         │
         │ CST                     │ AST
         ▼                         ▼
  ┌────────────────────────────────────────┐
  │      Reconciliation Layer              │
  │                                        │
  │  • Tree-sitter CST → Highlighting     │
  │  • Pest AST → Execution               │
  │  • Combined insights → Error messages │
  └────────────────────────────────────────┘
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
  → If Pest fails → show detailed error
  → If success → execute
```

### Execution Flow
```
1. User submits command
2. Optional: Check Tree-sitter CST (quick sanity check)
3. Parse with Pest (strict validation)
4. If Pest fails → show error with Tree-sitter context
5. If success → Execute AST with colony-exec
```

## Reconciliation Strategy

### Phase 1: Editor Features Only (Current)
- Tree-sitter active for syntax highlighting
- Pest used for execution
- Minimal reconciliation (separate concerns work independently)

### Phase 2: Combined Error Messages (Next)
- Tree-sitter CST shows structural issues
- Pest provides precise error location and message
- Combine insights for better user experience:
  - Tree-sitter: "incomplete if_statement node"
  - Pest: "expected 'then' after 'if' condition at line 2:15"
  - Combined: Detailed error with structural context

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
// Parse with Pest (only execution parser)
match pest_parse(source) {
    Ok(ast) => execute(ast),
    Err(pest_err) => {
        // Get Tree-sitter CST for context
        let cst = tree_sitter_parse(source);

        // Combine Pest error with Tree-sitter structural info
        show_combined_error(pest_err, cst)
    }
}

fn show_combined_error(pest_err: PestError, cst: Tree) {
    // Use Tree-sitter to understand what user was trying to do
    let incomplete_nodes = find_error_nodes(&cst);

    // Combine with Pest's precise error location
    eprintln!("Syntax error at {}", pest_err.location());
    eprintln!("{}", pest_err.message());

    if !incomplete_nodes.is_empty() {
        eprintln!("\nTree-sitter detected: {}", describe_nodes(&incomplete_nodes));
    }
}
```

## Implementation Status

- ✓ Tree-sitter grammar (forked from bash)
- ✓ Typed pipes added (-> => ~>)
- ✓ Bash grammar validates and parses correctly
- ✓ Highlighting queries written
- ⏳ Pest parser (F021 - 60 tests, nearly complete)
- ⚠️ Hand-written parser (F001/F002 - deprecated)
- ⏳ Reconciliation layer (design complete, impl pending)

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
