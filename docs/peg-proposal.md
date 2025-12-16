# Proposal: PEG-Based Parser with Custom Shell Handling

## Executive Summary

This proposal outlines a strategy to move Colony Shell's parser from a pure hand-written recursive descent parser to a **hybrid architecture** combining:

1. **PEG grammar** (`colony-core.peg`) for the core syntactic structure
2. **Custom preprocessing** for context-sensitive lexing (mode stack)
3. **Post-parse expansion** for shell word expansions
4. **Specialized handlers** for shell-specific edge cases

**Goal**: Maintain POSIX compliance and shell semantics while gaining PEG benefits:
- Declarative grammar specification
- Easier maintenance and evolution
- Better error recovery
- Automated test generation from grammar

## Background: Why Shells Are Hard for PEGs

Standard PEG parsers assume **context-free tokenization** where each character sequence maps to exactly one token type. Shell languages violate this assumption in several fundamental ways:

### Problem 1: Context-Sensitive Tokens

```bash
# '<' can be:
if [ $x < 10 ]; then     # String literal (in test command)
if (x < 10) {            # Less-than operator (expression mode)
cmd < file.txt           # Input redirection (command mode)
```

The meaning of `<` depends on the **parsing context** (inside `[ ]`, inside `()`, or command position).

### Problem 2: Barewords vs Identifiers

```bash
echo hello world         # 'hello' and 'world' are literal strings (command mode)
var x = hello           # 'hello' is an undefined identifier (expression mode)
var x = "hello"         # Correct: explicit string
```

Barewords are **strings in command mode** but **identifiers in expression mode**.

### Problem 3: Quote Interpolation

```bash
echo "Count: $count"           # $count expands to variable value
echo 'Count: $count'           # $count is literal text
echo "Items: $(ls *.txt)"      # Command substitution
echo "Math: $[2 + 2]"          # Expression substitution
```

Double quotes allow recursive expansion of `$var`, `$(cmd)`, and `$[expr]`. The lexer must recursively parse the contents while tracking quote nesting.

### Problem 4: Heredoc Lookahead

```bash
cat <<EOF | grep pattern    # Heredoc delimiter on this line
This is the heredoc         # Content starts on NEXT line
content here                # More content
EOF                         # Delimiter must be on its own line
```

The heredoc **content** doesn't start until after the entire command line is parsed. The lexer must:
1. Note the delimiter (`EOF`) when it sees `<<`
2. Continue parsing the rest of the command line
3. After the newline, enter heredoc mode and read until delimiter

### Problem 5: Operator Overloading by Position

```bash
echo foo | bar              # | is pipe operator
expr=$((a | b))             # | is bitwise OR
if [ $x -eq 5 ]; then       # [ is command name
if (x == 5) {               # ( is expression grouping
```

The same character sequence means different things based on **syntactic position**.

## Architecture: Three-Stage Pipeline

```
┌─────────────────┐
│  Source Text    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ 1. Preprocessor │ ◄─── Mode Stack Lexer
│   (Custom)      │      - Command/Expression modes
└────────┬────────┘      - Quote handling
         │               - Heredoc buffering
         │               - Token classification
         ▼
┌─────────────────┐
│ 2. PEG Parser   │ ◄─── Generated from colony-core.peg
│   (rust-peg)    │      - Syntactic structure
└────────┬────────┘      - Precedence and associativity
         │               - AST construction
         │
         ▼
┌─────────────────┐
│ 3. AST Expander │ ◄─── Expansion Engine
│   (Custom)      │      - Word expansion
└────────┬────────┘      - Parameter substitution
         │               - Command substitution
         ▼               - Glob expansion
   ┌─────────┐
   │   AST   │
   └─────────┘
```

## Stage 1: Preprocessing (Custom Lexer)

**Purpose**: Handle context-sensitive tokenization that standard lexers can't do.

### Mode Stack Implementation

The preprocessor maintains a **mode stack** that tracks the current lexing context:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LexerMode {
    Command,           // Barewords are strings, < is redirect
    Expression,        // Barewords are identifiers, < is comparison
    DoubleQuote,       // Allow $expansions, track nesting
    SingleQuote,       // Everything is literal
    HereDoc(HereDocState), // Read until delimiter
    Arithmetic,        // Inside $((expr))
}

pub struct Preprocessor {
    mode_stack: Vec<LexerMode>,
    pending_heredocs: Vec<HereDocDelimiter>,
    // ...
}
```

### Mode Transitions

The preprocessor pushes/pops modes based on syntax:

| Trigger | Action | Exit Condition |
|---------|--------|----------------|
| `$(` | Push `Command` | Matching `)` |
| `$[` | Push `Expression` | Matching `]` |
| `"` | Push `DoubleQuote` | Unescaped `"` |
| `'` | Push `SingleQuote` | Unescaped `'` |
| `if (` | Push `Expression` | Matching `)` |
| `<<DELIM` | Queue heredoc | After newline, push `HereDoc` |

### Token Stream Output

The preprocessor emits a **mode-aware token stream**:

```rust
pub struct PreprocessedToken {
    pub kind: TokenKind,
    pub value: TokenValue,
    pub span: Span,
    pub mode: LexerMode,  // Mode when token was lexed
}
```

The PEG parser receives this annotated stream where:
- Barewords in `Command` mode are already classified as `WORD`
- Barewords in `Expression` mode are classified as `IDENTIFIER`
- `<` in command mode is `REDIRECT_IN`
- `<` in expression mode is `LT`

### Heredoc Handling

Heredocs are **eagerly buffered** during preprocessing:

```rust
impl Preprocessor {
    fn handle_heredoc_operator(&mut self, delimiter: &str, strip_tabs: bool) {
        // 1. Queue the heredoc for later
        self.pending_heredocs.push(HereDocDelimiter {
            delimiter: delimiter.to_string(),
            strip_tabs,
        });

        // 2. Continue lexing the rest of the command line
        // ...
    }

    fn read_heredoc_content(&mut self, delimiter: &str) -> String {
        // 3. After newline, read lines until delimiter
        let mut content = String::new();
        loop {
            let line = self.read_line();
            if line.trim() == delimiter {
                break;
            }
            content.push_str(&line);
        }
        content
    }
}
```

The PEG parser receives a `HEREDOC_CONTENT(String)` token with the **entire content** already buffered.

## Stage 2: PEG Parsing

**Purpose**: Parse the syntactic structure using declarative grammar rules.

### What PEG Handles Well

PEG excels at:
- **Precedence climbing**: Expression operators with correct associativity
- **Greedy matching**: Longest match wins (no ambiguity)
- **Ordered choice**: Try alternatives in order, take first match
- **Lookahead**: Can check what's ahead without consuming
- **Error recovery**: Built-in backtracking for better error messages

### Colony Core Grammar

The PEG defines the **shape** of valid programs:

```peg
Statement
  = VarDecl
  / FunctionDef
  / IfStatement
  / Pipeline
  / ...

Pipeline
  = negation:"!"? _ elements:PipelineElements _

PipelineElements
  = first:PipelineElement rest:(_ PipeOp _ PipelineElement)* _
```

### Expression Precedence (Handled by PEG)

The PEG naturally encodes precedence through grammar nesting:

```peg
Expression
  = TernaryExpr                        # Lowest precedence

TernaryExpr
  = cond:NullCoalesceExpr
    rest:(_ "?" _ Expression _ ":" _ Expression)? _

NullCoalesceExpr
  = first:OrExpr rest:(_ "??" _ OrExpr)* _

OrExpr
  = first:AndExpr rest:(_ "||" _ AndExpr)* _

# ... down to ...

PrimaryExpr
  = Null / Boolean / Integer / ...     # Highest precedence
```

This is **much cleaner** than maintaining precedence manually in recursive descent.

### What PEG Cannot Handle

PEG cannot handle:
- **Infinite recursion in quotes**: `"outer $(echo "inner")"` requires unbounded nesting
- **Delayed heredoc content**: Content that appears after the command line
- **Expansion-dependent syntax**: `${#var}` length vs `${var#pattern}` strip prefix

These are handled by preprocessing (stage 1) and expansion (stage 3).

## Stage 3: Post-Parse Expansion

**Purpose**: Transform AST nodes containing `$expansions` into their expanded forms.

### Expansion Pipeline (POSIX Order)

```rust
pub fn expand_word(word: &Word, ctx: &mut ExpansionContext) -> Vec<String> {
    let intermediate = vec![IntermediateWord::from_word(word)];

    let intermediate = expand_tilde(intermediate);
    let intermediate = expand_parameter(intermediate, ctx);
    let intermediate = expand_command_substitution(intermediate, ctx);
    let intermediate = expand_arithmetic(intermediate, ctx);
    let intermediate = field_split(intermediate, ctx.ifs);
    let intermediate = expand_pathname(intermediate);
    let intermediate = remove_quotes(intermediate);

    intermediate.into_iter().map(|w| w.to_string()).collect()
}
```

### Parameter Expansion

The PEG parses `${var:-default}` into an AST node:

```rust
VariableRef {
    name: "var",
    modifier: Some(VarModifier::Default {
        word: Word { parts: vec![WordPart::Literal("default")] },
        check_null: true,
    }),
}
```

The expansion engine evaluates this at runtime:

```rust
fn expand_parameter(var_ref: &VariableRef, ctx: &ExpansionContext) -> String {
    let value = ctx.vars.get(&var_ref.name);

    match &var_ref.modifier {
        Some(VarModifier::Default { word, check_null }) => {
            if value.is_none() || (*check_null && value == Some("")) {
                expand_word(word, ctx).join(" ")
            } else {
                value.unwrap_or("")
            }
        }
        // ... handle other modifiers
    }
}
```

### Command Substitution

`$(cmd)` is parsed as:

```rust
WordPart::CommandSub(Box<Statement>)
```

Expansion executes the command and captures output:

```rust
fn expand_command_sub(stmt: &Statement, ctx: &ExpansionContext) -> String {
    let output = ctx.executor.run_capturing_output(stmt)?;
    output.trim_end_matches('\n')  // Strip trailing newlines (POSIX)
}
```

### Field Splitting

After expansion, `IntermediateWord` tracks quoted/unquoted regions:

```rust
struct IntermediateWord {
    parts: Vec<IntermediatePart>,
}

enum IntermediatePart {
    Quoted(String),     // Don't split
    Unquoted(String),   // Split on IFS
}
```

Field splitting only affects **unquoted** expansions:

```bash
var="a b c"
echo $var          # Splits: 3 args: 'a', 'b', 'c'
echo "$var"        # No split: 1 arg: 'a b c'
```

## Implementation Plan

### Phase 1: Generate pest Parser (2-3 days)

**Decision**: Using `pest` parser generator for its excellent Rust integration and error messages.

1. Add `pest` and `pest_derive` dependencies
2. Test grammar against `colony.pest`
3. Generate initial parser skeleton using `#[derive(Parser)]`
4. Add span tracking via pest's `Pair` API

### Phase 2: Integrate Preprocessor (3-5 days)

1. Adapt existing lexer as preprocessor
2. Emit mode-aware token stream
3. Connect preprocessor output to PEG input
4. Test with POSIX shell examples

### Phase 3: Keep Expansion Engine (1-2 days)

1. Extract expansion code from current parser (F003)
2. Ensure it operates on new AST types
3. Add tests for all expansion modifiers

### Phase 4: Validation (2-3 days)

1. Run all existing parser tests (currently 28 passing)
2. Add new tests for edge cases
3. Benchmark parsing performance
4. Compare AST output with recursive descent version

### Phase 5: Documentation (1 day)

1. Update parser documentation
2. Add grammar evolution guide
3. Document preprocessing rules

**Total estimated effort**: 9-14 days

## Benefits of This Approach

### 1. Declarative Grammar

The PEG is **self-documenting** and **version-controllable**:

```diff
+ ForStatement
+   = "for" _ kind:ForKind _ body:Block
+
+ ForKind
+   = idx:Identifier _ "," _ var:Identifier _ "in" _ "enumerate" ...
```

Adding a new construct just means adding grammar rules.

### 2. Easier Evolution

Colony will add many features:
- `@agent` definitions with complex DSL
- Type annotations with generics
- Pattern matching with guards
- Channel operations

A PEG scales better than hand-written code for this complexity.

### 3. Better Error Recovery

PEG generators include error recovery:

```
Error at line 12, column 5:
  if (x > 10 {
             ^
  Expected ')' or comparison operator
```

This is **automatic** with PEG, but requires manual engineering in recursive descent.

### 4. Formal Specification

The PEG **is** the spec. We can:
- Generate railroad diagrams
- Auto-generate test cases
- Prove properties about the grammar
- Share with other implementations

### 5. Maintainability

Recursive descent parsers grow to thousands of lines:

```rust
// Current approach: F002 parser is 800+ lines
fn parse_if_statement(&mut self) -> Result<IfStatement> {
    self.expect(TokenKind::If)?;
    let condition = self.parse_condition()?;
    if self.peek_kind() == TokenKind::Then {
        self.advance();
    }
    let then_body = self.parse_body()?;
    // ... 50 more lines ...
}
```

PEG is **10x more concise**:

```peg
IfStatement
  = "if" _ cond:Condition _ ("then" _)? then_body:Block
    elif:(__ "elif" _ Condition _ ("then" _)? Block)*
    else:(__ "else" _ Block)?
    _ ("fi" _)?
```

## Risks and Mitigations

### Risk 1: PEG Performance

**Risk**: Generated parsers may be slower than hand-written.

**Mitigation**:
- Benchmark early (Phase 4)
- Preprocessor already does heavy lifting (mode stack)
- Expression parsing is O(n) in both approaches
- If too slow, keep recursive descent for hot paths

### Risk 2: PEG Generator Limitations

**Risk**: Rust PEG generators may not support all features.

**Mitigation**:
- **Chose `pest`** for its excellent error messages and IDE support
- `pest` generates parse trees (not AST directly) giving us full control
- Mature ecosystem: `pest_derive`, `pest_fmt`, `pest_vm`
- Can always fall back to hand-written for specific constructs

### Risk 3: Loss of Control

**Risk**: Generated code is a black box.

**Mitigation**:
- Keep preprocessing custom (we control mode stack)
- Keep expansion custom (POSIX-specific semantics)
- PEG only handles **syntax**, not **semantics**

### Risk 4: POSIX Compatibility

**Risk**: Subtle differences from bash/dash behavior.

**Mitigation**:
- Extensive test suite comparing with bash (F001-F004 tests)
- POSIX test suite from bash/dash projects
- Preprocessing ensures shell-specific tokenization

## Alternatives Considered

### Alternative 1: Pure Recursive Descent (Current)

**Pros**: Full control, already working.

**Cons**: Hard to maintain, error-prone, 800+ lines per feature.

**Verdict**: Doesn't scale for Colony's ambitions (agents, types, channels, etc.).

### Alternative 2: Full PEG (No Preprocessing)

**Pros**: Simplest approach.

**Cons**: Cannot handle context-sensitive tokens, quote interpolation, or heredocs.

**Verdict**: Fundamentally incompatible with shell semantics.

### Alternative 3: Parser Combinators (nom, combine)

**Pros**: Flexible, composable.

**Cons**: Still requires manual mode tracking, no declarative grammar.

**Verdict**: Same maintenance burden as recursive descent, but less familiar.

### Alternative 4: Tree-sitter

**Pros**: Incremental parsing, excellent editor support.

**Cons**: C-based, LR parsing doesn't match PEG strengths, harder to embed custom preprocessing.

**Verdict**: Better for editors than execution engines.

## Recommendation

**Adopt the three-stage pipeline**:
1. Custom preprocessor (mode stack lexer) — **keep current approach**
2. PEG parser (generated from grammar) — **new, adds maintainability**
3. Custom expansion (word processing) — **keep current approach**

This gives us:
- **POSIX compliance** (preprocessing handles shell weirdness)
- **Maintainability** (PEG for syntactic structure)
- **Extensibility** (easy to add Colony features)
- **Performance** (preprocessing does heavy lifting once)

## Next Steps

1. **Review this proposal** with stakeholders
2. **Prototype Phase 1** (generate PEG parser from grammar)
3. **Validate** against F001-F004 tests
4. **Decide**: Proceed or refine approach

---

## Appendix A: PEG vs EBNF

The spec uses EBNF, but PEG is better for implementation:

| Feature | EBNF | PEG |
|---------|------|-----|
| Ambiguity | Possible (needs disambiguation rules) | None (ordered choice) |
| Left recursion | Requires special handling | Not allowed (use iteration) |
| Backtracking | Implementation-defined | Built-in |
| Error recovery | Manual | Automatic |
| Code generation | Multiple tools | rust-peg, pest |

Colony PEG uses **right recursion + iteration** for lists:

```peg
# EBNF: command_list = pipeline { list_op pipeline }
# PEG: Right-recursive with iteration
CommandList
  = first:Pipeline rest:(_ ListOp _ Pipeline)* _
```

## Appendix B: Handling Edge Cases

### Edge Case 1: `$[]` vs `$(())`

```bash
echo $[2+2]      # Colony expression substitution
echo $((2+2))    # POSIX arithmetic expansion
```

**Solution**: Preprocessor distinguishes:
- `$[` → `DOLLAR_BRACKET` token, push `Expression` mode
- `$((` → `DOLLAR_DOUBLE_PAREN` token, push `Arithmetic` mode

### Edge Case 2: Quote Nesting

```bash
echo "outer $(echo "inner $(echo "deep")")"
```

**Solution**: Mode stack tracks depth:
```
Command → DoubleQuote → Command (inside $()) → DoubleQuote → Command → DoubleQuote
```

Each `"` pops if it matches the top mode, otherwise pushes.

### Edge Case 3: Here-doc with Expansion

```bash
cat <<EOF
Value is $var
EOF
```

**Solution**: Preprocessor has two sub-modes:
- `HereDoc { expand: true }` → Process `$var` in content
- `HereDoc { expand: false }` → Literal content (delimiter was quoted: `<<'EOF'`)

## Appendix C: Performance Considerations

### Memory Usage

- **Preprocessing**: Single-pass, streams tokens
- **PEG**: Memoization may cache partial results
- **Expansion**: Builds intermediate representation

**Estimate**: ~2x memory vs current (acceptable for shell).

### Parse Time

- **Preprocessing**: O(n) — same as current lexer
- **PEG**: O(n) to O(n²) depending on backtracking
- **Expansion**: O(n × expansion_factor)

**Estimate**: Similar to current, possibly 10-20% slower.

### Optimization Opportunities

1. **Packrat parsing**: Memoize PEG results (built into `rust-peg`)
2. **Token buffering**: Reduce allocation in preprocessor
3. **Lazy expansion**: Only expand words when executed
4. **Parallel parsing**: Multiple PEG parsers for functions/subshells

## Appendix D: pest Integration Guide

### Why pest?

`pest` is the best choice for Colony Shell because:

1. **Excellent error messages**: pest automatically generates helpful error messages with line/column information
2. **Parse tree control**: pest generates a parse tree (not AST), giving us full control over AST construction
3. **IDE support**: VSCode extensions for syntax highlighting and testing
4. **Zero-copy parsing**: pest uses string slices (`&str`) throughout, no allocations during parsing
5. **Mature ecosystem**: `pest_derive` for macro-based parser generation, `pest_fmt` for formatting grammars

### Basic Integration

**Cargo.toml**:
```toml
[dependencies]
pest = "2.7"
pest_derive = "2.7"
```

**Parser structure**:
```rust
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "../docs/colony.pest"]
pub struct ColonyParser;

pub fn parse_program(source: &str) -> Result<Program, ParseError> {
    // 1. Preprocess (custom mode-stack lexer)
    let tokens = preprocess(source)?;

    // 2. Parse with pest
    let pairs = ColonyParser::parse(Rule::Program, &tokens)?;

    // 3. Build AST from parse tree
    let ast = build_ast(pairs)?;

    Ok(ast)
}
```

### Parse Tree to AST

pest generates a `Pairs` iterator over matched rules:

```rust
fn build_statement(pair: Pair<Rule>) -> Result<Statement, ParseError> {
    match pair.as_rule() {
        Rule::IfStatement => {
            let mut inner = pair.into_inner();

            // Get condition
            let cond_pair = inner.next().unwrap();
            let condition = build_condition(cond_pair)?;

            // Get then body
            let then_pair = inner.next().unwrap();
            let then_body = build_block(then_pair)?;

            // Optional elif/else branches
            let mut elif_branches = Vec::new();
            let mut else_body = None;

            for pair in inner {
                match pair.as_rule() {
                    Rule::ElifBranch => {
                        elif_branches.push(build_elif(pair)?);
                    }
                    Rule::ElseBranch => {
                        else_body = Some(build_block(pair)?);
                    }
                    _ => unreachable!(),
                }
            }

            Ok(Statement::If(IfStatement {
                condition,
                then_body,
                elif_branches,
                else_body,
                span: span_from_pair(&pair),
            }))
        }
        // ... other statement types
    }
}
```

### Span Tracking

pest provides built-in span information:

```rust
fn span_from_pair(pair: &Pair<Rule>) -> Span {
    let span = pair.as_span();
    Span {
        start: Position {
            offset: span.start(),
            line: span.start_pos().line_col().0 as u32,
            column: span.start_pos().line_col().1 as u32,
        },
        end: Position {
            offset: span.end(),
            line: span.end_pos().line_col().0 as u32,
            column: span.end_pos().line_col().1 as u32,
        },
        source_id: None,
    }
}
```

### Error Handling

pest provides detailed error messages automatically:

```
Error: expected Expression
  --> script.col:5:8
   |
 5 | var x =
   |        ^
   |
   = expected Expression
```

Customize with `pest::error::Error::with_path`:

```rust
match ColonyParser::parse(Rule::Program, source) {
    Ok(pairs) => build_ast(pairs),
    Err(e) => {
        let error = e.with_path("script.col");
        eprintln!("{}", error);
        Err(ParseError::Syntax(error))
    }
}
```

### Testing Grammar

Use `pest_fmt` to format and validate:

```bash
cargo install pest_fmt
pest_fmt docs/colony.pest
```

VSCode extension: `pest-ide-tools`

### Performance Optimization

1. **Atomic rules**: Use `@` prefix for tokens that don't need parse tree nodes
   ```pest
   Integer = @{ ASCII_DIGIT+ }  // No intermediate nodes
   ```

2. **Silent rules**: Use `_` prefix for rules that don't need to appear in output
   ```pest
   WHITESPACE = _{ " " | "\t" }  // Handled automatically
   ```

3. **Packrat parsing**: pest uses memoization automatically

---

**Authors**: Colony Shell Team
**Date**: 2025-12-16
**Status**: Proposed
**Decision**: Using `pest` parser generator
