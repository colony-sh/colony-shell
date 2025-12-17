# Combined Parser Work - pest + Tree-sitter

**Status**: F021 (pest) 90% complete, F022 (tree-sitter) structure ready
**Last Updated**: 2025-12-16

---

## Architecture Overview

Colony Shell uses **two complementary parsers**:

| Parser | Purpose | Status | Tests |
|--------|---------|--------|-------|
| **pest PEG** | Execution (strict) | 90% | 60 passing |
| **Tree-sitter** | Editor (tolerant) | Structure ready | Not integrated |

---

## Critical Path Dependencies

```
pest Expression Operators (2-3d)
  ↓
pest Integration with colony-exec (1d)
  ↓
pest Heredocs (1-2d)
  ‖  (parallel)
Tree-sitter rust-sitter generation (1d)
  ↓
Tree-sitter Integration with REPL (2-3d)
  ↓
Reconciliation Layer (3-4d)
```

**Total Critical Path**: ~10-14 days

---

## F021: pest PEG Parser (Execution)

### Phase 1: Expression Operators [HIGH PRIORITY]
**Status**: Grammar complete, builder needs implementation
**Effort**: 2-3 days
**Depends on**: Nothing (can start now)
**Tests**: Currently pass for literals, need operator tests

**Tasks**:
```
[P1.1] Implement binary operator building
  - Walk precedence levels (TernaryExpr → OrExpr → AndExpr → ...)
  - Left-associative: a + b + c → (a + b) + c
  - Right-associative: a ? b : c (ternary)

[P1.2] Handle all 15 precedence levels
  - Ternary (?:) and null coalesce (??)
  - Logical (&&, ||)
  - Comparison (<, >, ==, !=, =~, !~)
  - Bitwise (&, |, ^, <<, >>)
  - Arithmetic (+, -, *, /, %, **)
  - Unary (!, -, ~, not)

[P1.3] Implement postfix operators
  - Index: arr[i]
  - Field: obj.field
  - Call: func(args)
  - Optional chain: obj?.field
  - Unwrap: result!

[P1.4] Add comprehensive expression tests
  - Test all operator combinations
  - Test precedence: 2 + 3 * 4 → 2 + (3 * 4)
  - Test associativity: 1 - 2 - 3 → (1 - 2) - 3
  - Test short-circuit: a && b (don't eval b if a false)

[P1.5] Expression builder documentation
  - Document precedence climbing algorithm
  - Add examples for each operator
  - Note any deviations from POSIX
```

**Acceptance**: All arithmetic and logical expressions parse and build correctly

---

### Phase 2: Integration with Executor [HIGH PRIORITY]
**Status**: Not started
**Effort**: 1 day
**Depends on**: P1 (expressions)
**Blocks**: Real usage of pest parser

**Tasks**:
```
[P2.1] Update colony-exec to use pest_api::parse()
  - Replace: Lexer::new() + Parser::new() + parse()
  - With: colony_parser::parse(source)?
  - Update all call sites

[P2.2] Update colony-shell REPL
  - Use parse_statement() for line-by-line execution
  - Handle ParseError with nice formatting
  - Show error line/column visually

[P2.3] Update examples
  - parser_demo example to use new API
  - Add pest_demo example showing pure PEG

[P2.4] Deprecate F001/F002
  - Mark Lexer and Parser as #[deprecated]
  - Add deprecation notes pointing to pest_api
  - Keep code for reference but discourage use

[P2.5] Update benchmarks
  - Add pest vs recursive descent comparison
  - Measure parse speed improvements
  - Document in README
```

**Acceptance**: colony-exec and colony-shell use pest parser exclusively

---

### Phase 3: Heredoc Support [MEDIUM PRIORITY]
**Status**: Not started
**Effort**: 1-2 days
**Depends on**: P2 (integration)
**Blocks**: Complete POSIX compliance

**Tasks**:
```
[P3.1] Design heredoc preprocessing strategy
  - Option A: Text normalization before pest
  - Option B: Two-pass (find markers, extract, parse)
  - Option C: Post-parse content injection
  - Recommend: Option A (minimal preprocessing)

[P3.2] Implement heredoc extractor
  - Find <<DELIM and <<-DELIM in source
  - Extract content from following lines
  - Create normalized source: cat <<EOF{CONTENT}EOF
  - Handle quoted delimiters: <<'EOF' (no expansion)

[P3.3] Update grammar for normalized heredocs
  - HereDoc = "<<" ~ Identifier ~ "{" ~ HereDocContent ~ "}" ~ Identifier
  - HereDocContent = @{ (!("}" ~ Identifier) ~ ANY)* }
  - Preserve semantics (strip tabs for <<-)

[P3.4] Integrate into parse() function
  fn parse(source: &str) -> Result<Program> {
      let normalized = preprocess_heredocs(source)?;
      let pairs = ColonyParser::parse(Rule::Program, &normalized)?;
      AstBuilder::build_program(pairs)
  }

[P3.5] Test all heredoc variants
  - << (normal)
  - <<- (strip tabs)
  - <<< (here-string)
  - Quoted delimiter (no expansion)
  - Nested in command substitution
```

**Acceptance**: All POSIX heredoc forms work correctly

---

### Phase 4: Word-List For Loops [LOW PRIORITY]
**Status**: Grammar issue (C-style works)
**Effort**: 1 day
**Depends on**: Nothing (grammar fix)
**Blocks**: Full POSIX for loop support

**Tasks**:
```
[P4.1] Fix "in" keyword ambiguity
  - Currently: "in" is in ReservedWord
  - Problem: ForKind can't match "in" as keyword
  - Solution: Make "in" context-specific

[P4.2] Update ForKind grammar
  ForKind = {
      Identifier ~ "," ~ Identifier ~ "in" ~ "enumerate" ~ "(" ~ Expression ~ ")"
    | "(" ~ Expression? ~ ";" ~ Expression? ~ ";" ~ Expression? ~ ")"
    | Identifier ~ @{ "in" } ~ WordList  // Force "in" to match here
    | Identifier ~ @{ "in" } ~ Expression
  }

[P4.3] Test word-list for loops
  - for file in *.txt; do echo $file; done
  - for x in word1 word2 word3; do echo $x; done
  - for item in $(ls); do process $item; done

[P4.4] Update builder for word-list variant
  - Currently handles C-style only
  - Add WordList parsing in build_for_statement()
```

**Acceptance**: `for var in words` parses correctly

---

## F022: Tree-sitter Parser (Editor)

### Phase 5: rust-sitter Generation [HIGH PRIORITY]
**Status**: C parser exists, need Rust generation
**Effort**: 1 day
**Depends on**: Nothing (independent of pest)
**Blocks**: Pure Rust integration

**Tasks**:
```
[T5.1] Install rust-sitter CLI
  cargo install rust-sitter-cli
  rust-sitter --version

[T5.2] Generate Rust parser from grammar
  cd tree-sitter-colony
  npx tree-sitter generate  # Update grammar.json
  cd bindings/rust
  rust-sitter generate --rust-package-info Cargo.toml

[T5.3] Verify generated parser.rs
  - Check bindings/rust/src/parser.rs created
  - Should be ~5-10k lines of Rust
  - No unsafe blocks (pure Rust)

[T5.4] Remove C artifacts
  - Keep src/parser.c for reference (commented)
  - Remove from build (no build.rs)
  - Update Cargo.toml (no cc dependency)

[T5.5] Test Rust parser
  cargo test -p tree-sitter-colony
  - Should pass 4 tests
  - Load grammar
  - Parse simple command
```

**Acceptance**: Pure Rust tree-sitter parser compiles and passes tests

---

### Phase 6: Tree-sitter Integration [HIGH PRIORITY]
**Status**: Not started
**Effort**: 2-3 days
**Depends on**: T5 (rust-sitter generation)
**Blocks**: Real-time syntax highlighting

**Tasks**:
```
[T6.1] Add tree-sitter-colony to workspace
  - Add to workspace members in root Cargo.toml
  - Update colony-shell dependencies
  - Ensure compilation works

[T6.2] Create tree-sitter wrapper in colony-shell
  // crates/colony-shell/src/tree_sitter_parser.rs
  pub struct EditorParser {
      parser: tree_sitter::Parser,
      prev_tree: Option<tree_sitter::Tree>,
  }

  impl EditorParser {
      pub fn parse_incremental(&mut self, source: &str) -> Tree {
          let tree = self.parser.parse(source, self.prev_tree.as_ref())?;
          self.prev_tree = Some(tree.clone());
          tree
      }
  }

[T6.3] Integrate into REPL
  - Parse on every keystroke (incremental)
  - Extract highlights from CST
  - Emit OSC 2337 escape sequences to terminal

[T6.4] Implement highlight extraction
  pub fn extract_highlights(tree: &Tree, source: &str) -> Vec<Highlight> {
      let query = Query::new(language(), HIGHLIGHTS_QUERY)?;
      let mut cursor = QueryCursor::new();
      cursor.matches(&query, tree.root_node(), source.as_bytes())
          .flat_map(|m| m.captures.iter())
          .map(|c| Highlight {
              range: c.node.byte_range(),
              kind: capture_name(c.index),
          })
          .collect()
  }

[T6.5] Test incremental parsing
  - Measure: < 1ms per edit
  - Test: typing "echo hello" character by character
  - Verify: highlights update instantly
```

**Acceptance**: Syntax highlighting works in real-time during typing

---

### Phase 7: Highlighting Queries [MEDIUM PRIORITY]
**Status**: Basic queries exist, need refinement
**Effort**: 1-2 days
**Depends on**: T6 (integration)

**Tasks**:
```
[T7.1] Review existing highlights.scm
  - Check coverage of all constructs
  - Add missing patterns

[T7.2] Add semantic highlighting
  - Variables: @variable
  - Functions: @function
  - Commands: @function.builtin vs @function.external
  - Keywords: @keyword.control
  - Operators: @operator

[T7.3] Add Colony-specific highlights
  - Typed pipes: ->, => as @operator.typed
  - Agent definitions: @keyword.agent
  - Tuple operations: @function.builtin.tuple
  - Memory operations: @function.builtin.memory

[T7.4] Test highlighting in terminal
  - Use OSC 2337 to send highlights
  - Verify terminal renders colors correctly
  - Test all token types have colors

[T7.5] Add color scheme defaults
  - Define default colors for each token type
  - Document in queries/README.md
```

**Acceptance**: All language constructs have appropriate syntax highlighting

---

### Phase 8: Reconciliation Layer [LOW PRIORITY]
**Status**: Documented, not implemented
**Effort**: 3-4 days
**Depends on**: P2 (pest exec), T6 (tree-sitter)

**Tasks**:
```
[T8.1] Create colony-reconcile crate
  mkdir crates/colony-reconcile
  - Depends on: tree-sitter-colony, colony-parser

[T8.2] Implement error reconciliation
  pub fn reconcile_errors(
      tree_sitter_tree: &Tree,
      pest_error: &ParseError,
      source: &str,
  ) -> EnhancedError {
      // Use Tree-sitter CST for context
      // Use Pest error for precise message
      // Combine into rich diagnostic
  }

[T8.3] Implement quick fixes
  - Tree-sitter identifies error node
  - Generate fix suggestions
  - "Did you mean: fi instead of done?"

[T8.4] Implement structure queries
  - Find all functions (Tree-sitter)
  - Extract function signatures (Pest for precision)
  - Combined: accurate function list

[T8.5] Documentation and examples
  - API docs for reconcile_errors()
  - Examples of error improvements
  - Perf benchmarks
```

**Acceptance**: Errors show both Tree-sitter context and pest precision

---

## Shared Work (Both Parsers)

### Phase 9: Grammar Synchronization [ONGOING]
**Status**: Both grammars exist, need alignment
**Effort**: Ongoing
**Priority**: MEDIUM

**Tasks**:
```
[S9.1] Compare grammar coverage
  - List constructs in colony.pest
  - List constructs in grammar.js
  - Identify differences

[S9.2] Align constructs
  - Ensure both parse same syntax
  - Tree-sitter can be more permissive (error recovery)
  - Pest must be strict (execution validation)

[S9.3] Document differences
  - Which constructs only in Tree-sitter (error recovery)
  - Which only in Pest (strict validation)
  - Why they differ

[S9.4] Create shared test corpus
  - Test cases both parsers should handle
  - Run against both: tree-sitter test and cargo test
  - Ensure consistency

[S9.5] Version together
  - When adding new syntax, update both grammars
  - Use same version numbers
  - Document in CHANGELOG
```

**Acceptance**: Both grammars handle same Colony syntax

---

### Phase 10: Documentation [ONGOING]
**Status**: Good docs exist, need updates
**Effort**: 1-2 days
**Priority**: MEDIUM

**Tasks**:
```
[S10.1] Update CLAUDE.md
  - Remove F001/F002 from session protocol
  - Add: pest parser is primary execution parser
  - Add: Tree-sitter for editor features only

[S10.2] Update rules.txt
  - Update rule #12: "Parser must be hand-written" → OBSOLETE
  - Add rule: "pest grammar is source of truth for syntax"
  - Add rule: "Tree-sitter for editor, pest for execution"

[S10.3] Create grammar evolution guide
  - How to add new syntax to colony.pest
  - How to synchronize with grammar.js
  - Testing strategy (pest tests + tree-sitter tests)

[S10.4] Update PARSER_ARCHITECTURE.md
  - Reflect pure PEG breakthrough
  - Update three-parser to two-parser diagram
  - Remove F001/F002 references

[S10.5] Add migration guide for consumers
  - Old API: Lexer + Parser
  - New API: pest_api::parse()
  - Migration examples
```

**Acceptance**: Docs accurately reflect two-parser architecture

---

## Prioritized Task List (By Urgency)

### 🔴 Critical (Blocks Other Work)

1. **[P1] pest Expression Operators** (2-3 days)
   - Without this, can't evaluate arithmetic or conditions
   - Blocks executor integration
   - 15 precedence levels to implement

2. **[P2] pest Integration with Executor** (1 day)
   - Blocks real usage of pest parser
   - Simple API update: use parse() instead of Lexer+Parser
   - Enables deprecation of F001/F002

3. **[T5] rust-sitter Generation** (1 day)
   - Generates pure Rust parser
   - Blocks tree-sitter integration
   - Single command: rust-sitter generate

### 🟡 High Priority (Needed Soon)

4. **[T6] Tree-sitter Integration** (2-3 days)
   - Enables real-time syntax highlighting
   - Improves REPL UX significantly
   - Depends on T5

5. **[P3] pest Heredoc Support** (1-2 days)
   - Needed for POSIX compliance
   - Can do in parallel with T6
   - Minimal preprocessing only

### 🟢 Medium Priority (Nice to Have)

6. **[T7] Highlighting Queries** (1-2 days)
   - Improves syntax highlighting quality
   - Adds Colony-specific tokens
   - Can iterate on this

7. **[S9] Grammar Synchronization** (ongoing)
   - Ensures consistency
   - Important for maintenance
   - Ongoing as grammar evolves

8. **[P4] Word-List For Loops** (1 day)
   - Grammar fix for "in" keyword
   - POSIX compliance
   - Currently use C-style as workaround

### 🔵 Low Priority (Future)

9. **[T8] Reconciliation Layer** (3-4 days)
   - Enhanced error messages
   - Not blocking core functionality
   - Nice polish

10. **[S10] Documentation Updates** (1-2 days)
    - Can do incrementally
    - Important for team, not blocking

---

## Estimated Timeline

### Sprint 1 (Week 1): Get pest Fully Functional
```
Days 1-3: P1 - Expression operators
Day 4:    P2 - Executor integration
Days 5-6: P3 - Heredoc support
```
**Deliverable**: pest parser 100% complete, replacing F001/F002

### Sprint 2 (Week 2): Tree-sitter Integration
```
Day 1:    T5 - rust-sitter generation
Days 2-4: T6 - REPL integration
Days 5:   T7 - Highlight queries
```
**Deliverable**: Real-time syntax highlighting working

### Sprint 3 (Week 3): Polish & Reconciliation
```
Day 1:    P4 - Word-list for loops
Days 2-4: T8 - Reconciliation layer
Day 5:    S10 - Documentation
```
**Deliverable**: Complete two-parser system with error reconciliation

**Total**: ~15 days to fully complete both parsers

---

## Quick Wins (Can Do Today)

### Today #1: rust-sitter Generation (1 hour)
```bash
cargo install rust-sitter-cli
cd tree-sitter-colony
npx tree-sitter generate
cd bindings/rust
rust-sitter generate --rust-package-info Cargo.toml
cargo test
```
**Impact**: Immediate - pure Rust tree-sitter available

### Today #2: Start Expression Operators (3-4 hours)
```rust
// Implement binary operator climbing
fn build_additive_expr(pair: Pair<Rule>) -> Result<Expression> {
    let mut inner = pair.into_inner();
    let mut left = build_multiplicative(inner.next().unwrap())?;

    while let Some(part) = inner.next() {
        if part.as_str() == "+" || part.as_str() == "-" {
            let op = if part.as_str() == "+" { BinaryOp::Add } else { BinaryOp::Sub };
            let right = build_multiplicative(inner.next().unwrap())?;
            left = Expression::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span: ...,
            };
        }
    }
    Ok(left)
}
```
**Impact**: Medium - enables arithmetic evaluation

### Today #3: Deprecate F001/F002 (30 minutes)
```rust
#[deprecated(since = "0.2.0", note = "Use colony_parser::parse() instead")]
pub struct Lexer { ... }

#[deprecated(since = "0.2.0", note = "Use colony_parser::parse() instead")]
pub struct Parser { ... }
```
**Impact**: Low - documentation clarity

---

## Success Criteria

### pest Parser (F021)
- ✅ 60 tests passing (done!)
- ⏳ Expression operators working
- ⏳ Integrated with colony-exec
- ⏳ Heredocs supported
- ⏳ F001/F002 deprecated

### Tree-sitter (F022)
- ⏳ Pure Rust parser generated
- ⏳ Integrated with REPL
- ⏳ Syntax highlighting working
- ⏳ < 1ms incremental parsing

### Combined
- ⏳ Both parse same syntax
- ⏳ Error reconciliation working
- ⏳ Documentation complete

---

## Dependencies Graph

```
     P1 (expressions)
      ↓
     P2 (integration) ←─┐
      ↓                 │
     P3 (heredocs)      │
      ↓                 │
     P4 (for loops)     │
                        │
     T5 (rust-sitter) ──┘
      ↓
     T6 (REPL integration)
      ↓
     T7 (highlights)
      ↓
   ┌─ T8 (reconciliation) ←─┐
   │                        │
   │                        │
   └─→ S9 (sync grammars) ←─┘
       ↓
      S10 (docs)
```

**Critical Path**: P1 → P2 → T5 → T6 (8-9 days)
**Full Completion**: +P3, P4, T7, T8, S9, S10 (15 days total)

---

## Notes

### What We Learned

1. **pest doesn't need F001**: Grammar structure provides context
2. **Heredocs are special**: Only construct needing preprocessing
3. **Two parsers are complementary**: Tree-sitter for speed, pest for precision

### What Changed

- ~~Three-stage pipeline~~ (preprocess → parse → expand)
- Now: **Two-stage** (parse → expand) for most constructs
- Heredocs: Three-stage still (preprocess → parse → expand)

### Next Session

**Start with**: P1 (expression operators) - highest impact, no dependencies
**Then**: T5 (rust-sitter generate) - 1 hour, unlocks T6
**Finally**: P2 (integration) - connects everything

---

**Last Updated**: 2025-12-16
**Maintained by**: Colony Shell Team
**Questions**: See docs/peg-proposal.md, docs/PURE_PEG_FINDINGS.md
