# Colony Shell Grammar Design

## Overview

Colony Shell uses a **hybrid parsing architecture** that combines the strengths of Tree-sitter (incremental, error-tolerant) with Pest/PEG (precise semantics, unlimited lookahead).

```
┌─────────────────────────────────────────────────────────────────┐
│                         Source Code                              │
└─────────────────────────┬───────────────────────────────────────┘
                          │
          ┌───────────────┴───────────────┐
          ▼                               ▼
┌───────────────────────┐     ┌─────────────────────────┐
│    Tree-sitter        │     │      Pest (PEG)         │
│    (Incremental)      │     │      (Precise)          │
│                       │     │                         │
│ • Syntax highlighting │     │ • Execution semantics   │
│ • Structural editing  │     │ • Validation            │
│ • Error recovery      │     │ • AST generation        │
│ • IDE features        │     │ • Type checking         │
└───────────┬───────────┘     └───────────┬─────────────┘
            │                             │
            │   Concrete Syntax Tree      │   Abstract Syntax Tree
            │   (with ERROR nodes)        │   (validated)
            ▼                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Reconciliation Layer                          │
│                                                                  │
│  • Map Tree-sitter nodes to Pest AST                            │
│  • Propagate edits efficiently                                   │
│  • Cache Pest parse results by Tree-sitter subtree hash         │
└─────────────────────────────────────────────────────────────────┘
```

## Syntax Extensions

Colony extends POSIX shell with these constructs:

### 1. Extended I/O Channels

```colony
# Numbered channels beyond 0,1,2
cmd 3> file.txt           # Write channel 3 to file
cmd 4< input.txt          # Read channel 4 from file
cmd 3>> append.txt        # Append channel 3

# Named channels
cmd @context> ctx.json    # Named channel "context"
cmd @errors< err.log      # Named channel "errors"

# Cross-channel piping
cmd1 3|4 cmd2             # Channel 3 of cmd1 → channel 4 of cmd2

# Channel duplication
cmd 3>&1                  # Duplicate stdout to channel 3
cmd 4<&0                  # Duplicate stdin to channel 4
```

### 2. Buffer References

Buffers are in-memory storage accessible by number or name:

```colony
# Numeric buffers
echo "data" > $[0]        # Write to buffer 0
cat $[0]                  # Read from buffer 0
cmd < $[1] > $[2]         # Use buffers for I/O

# Named buffers
echo "context" > $[ctx]   # Write to named buffer
agent "analyze $[ctx]"    # Reference in command
```

### 3. Tuple Space Operations

Linda-style tuple space for multi-agent coordination:

```colony
# Write tuple to space
out { task: "analyze", data: $input, priority: 1 }

# Write to named space
out @tasks { id: "task-001", status: "pending" }

# Blocking read and remove (pattern matching with bindings)
in { task: "analyze", data: ?payload, priority: ?p }
# $payload and $p are now bound

# Blocking read without remove (peek)
rd { task: *, status: "pending" }

# Non-blocking variants (return immediately)
inp { task: "analyze" }   # Returns tuple or null
rdp { status: ?s }        # Peek without blocking

# With timeout
in { task: * } timeout 5s

# Pattern features
{ key: "literal" }        # Exact match
{ key: ?var }             # Bind to variable
{ key: * }                # Wildcard (match anything)
{ *: "value" }            # Wildcard key
{ nested: { inner: ?x } } # Nested patterns
```

### 4. Agent Commands

First-class agent invocation:

```colony
# Simple invocation
agent "Summarize this text"

# With options
agent -m claude-sonnet "Quick analysis"
agent --model=gpt4 --timeout=30 "Complex task"
agent --caps=fs,net "Task needing permissions"
agent --stream "Show output as it generates"

# Multi-line prompt block (with expansion)
agent {
    Analyze the following data:
    $[data_buffer]
    
    Context from: $(cat context.txt)
    
    Focus on: ${ANALYSIS_FOCUS:-general trends}
}

# Agent pipe operator
cat data.csv |> agent "Convert to JSON"
ls -la |> agent "Explain these permissions"

# Chained agents
cat log.txt |> agent "Extract errors" |> agent "Categorize by severity"
```

### 5. Reactive Bindings

Variables that automatically update:

```colony
# Reactive variable (re-evaluates when dependencies change)
let! summary = $(agent "summarize $[input]")

# Watch for changes
watch $[input] {
    echo "Input buffer changed, reprocessing..."
    process_data
}

watch $config {
    reload_configuration
}
```

### 6. Capability Blocks

Sandboxed execution with explicit permissions:

```colony
# Grant specific capabilities
with caps(fs:/home/user/data, net:api.example.com) {
    agent "Fetch data and save locally"
}

# Capability types:
# - fs:<path>     Filesystem access (glob patterns supported)
# - net:<host>    Network access (wildcards: *.example.com)
# - env:<var>     Environment variable access
# - exec:<cmd>    Command execution permission
# - agent         Permission to invoke agents

# Nested capability restriction
with caps(fs:/project) {
    with caps(fs:/project/src) {  # Further restricted
        agent "Modify source files only"
    }
}
```

## Grammar Design Decisions

### Tree-sitter Grammar (grammar.js)

**Goals:**
- Fast incremental re-parsing
- Graceful error recovery
- Rich CST for syntax highlighting and structural editing

**Key Design Choices:**

1. **External Scanner for Heredocs**
   ```javascript
   externals: $ => [
       $.heredoc_start,
       $.heredoc_body,
       $.heredoc_end,
       $._newline,
       $._concat,
   ]
   ```
   Heredocs require stateful scanning that PEG/CFG can't express cleanly.

2. **Precedence for Ambiguity Resolution**
   ```javascript
   pipeline: $ => prec.left(1, seq(
       $._statement,
       choice('|', '|&'),
       $._statement,
   ))
   ```
   Tree-sitter uses explicit precedence to resolve what PEG handles with ordered choice.

3. **Dynamic Precedence for Context-Sensitivity**
   ```javascript
   redirected_statement: $ => prec.dynamic(-1, seq(
       field('body', $._statement),
       repeat1($.redirect),
   ))
   ```
   Shell's context-sensitivity (is `for` a keyword or argument?) requires runtime disambiguation.

### Pest Grammar (colony.pest)

**Goals:**
- Precise, unambiguous semantics
- Rich pattern matching for AST construction
- Validation before execution

**Key Design Choices:**

1. **Ordered Choice for Disambiguation**
   ```pest
   statement = {
       capability_block |    // Most specific first
       tuple_operation |
       agent_command |
       // ... more specific ...
       command               // Most general last
   }
   ```
   PEG's ordered choice eliminates ambiguity by design.

2. **Negative Lookahead for Keywords**
   ```pest
   tuple_in = {
       "in" ~ !(")" | WHITESPACE* ~ ("then" | "do")) ~
       tuple_space_name? ~ tuple_pattern
   }
   ```
   Distinguishes `in` (tuple op) from `in` (for loop keyword) without ambiguity.

3. **Silent Rules for Noise**
   ```pest
   WHITESPACE = _{ " " | "\t" }
   COMMENT = _{ "#" ~ (!NEWLINE ~ ANY)* }
   ```
   Underscore prefix makes rules silent (not in parse tree), cleaning up the AST.

## Reconciliation Strategy

### Hash-Based Caching

Each Tree-sitter subtree has a stable hash. Use this to cache Pest parse results:

```rust
struct ReconciliationCache {
    // Tree-sitter node hash → Pest AST node
    cache: HashMap<u64, Arc<AstNode>>,
}

impl ReconciliationCache {
    fn get_or_parse(&mut self, ts_node: &Node, source: &str) -> Arc<AstNode> {
        let hash = ts_node.hash();
        
        if let Some(cached) = self.cache.get(&hash) {
            return cached.clone();
        }
        
        // Parse with Pest
        let text = &source[ts_node.byte_range()];
        let ast = pest_parse(text)?;
        
        self.cache.insert(hash, Arc::new(ast));
        ast
    }
}
```

### Incremental Validation

Only re-validate changed subtrees:

```rust
fn validate_incremental(
    old_tree: &Tree,
    new_tree: &Tree,
    edits: &[Edit],
    cache: &mut ReconciliationCache,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    
    // Find changed nodes
    let changed = find_changed_nodes(old_tree, new_tree, edits);
    
    for node in changed {
        // Invalidate cache for changed nodes
        cache.invalidate(node.hash());
        
        // Re-parse and validate
        match cache.get_or_parse(&node, source) {
            Ok(ast) => { /* valid */ }
            Err(e) => diagnostics.push(e.into()),
        }
    }
    
    diagnostics
}
```

### Error Recovery Mapping

Map Tree-sitter ERROR nodes to Pest parse failures:

```rust
fn map_errors(ts_tree: &Tree) -> Vec<SyntaxError> {
    let mut errors = Vec::new();
    let mut cursor = ts_tree.walk();
    
    loop {
        let node = cursor.node();
        
        if node.is_error() || node.is_missing() {
            errors.push(SyntaxError {
                range: node.byte_range(),
                message: infer_error_message(&node),
                recovery_hint: suggest_fix(&node),
            });
        }
        
        if !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                break;
            }
        }
    }
    
    errors
}
```

## Usage Patterns

### Interactive Shell (REPL)

```rust
struct ColonyRepl {
    ts_parser: TreeSitterParser,
    pest_validator: PestParser,
    cache: ReconciliationCache,
}

impl ColonyRepl {
    fn process_input(&mut self, line: &str) -> Result<()> {
        // Fast parse for immediate feedback
        let ts_tree = self.ts_parser.parse(line, None)?;
        
        // Check for errors
        if ts_tree.root_node().has_error() {
            return Err(self.format_syntax_error(&ts_tree));
        }
        
        // Full validation before execution
        let ast = self.pest_validator.parse(line)?;
        
        // Execute
        self.execute(ast)
    }
}
```

### IDE/Editor Integration

```rust
// On every keystroke: Tree-sitter only (fast)
fn on_change(&mut self, change: TextChange) {
    self.ts_tree = self.ts_parser.parse(
        &self.source,
        Some(&self.ts_tree),  // Incremental!
    );
    
    // Update highlighting immediately
    self.update_highlights(&self.ts_tree);
    
    // Schedule full validation (debounced)
    self.schedule_validation();
}

// Debounced: Full Pest validation
fn validate(&mut self) {
    match self.pest_parser.parse(&self.source) {
        Ok(ast) => {
            self.clear_diagnostics();
            self.ast = Some(ast);
        }
        Err(e) => {
            self.show_diagnostic(e);
        }
    }
}
```

### Batch Execution (Scripts)

```rust
fn execute_script(path: &Path) -> Result<()> {
    let source = std::fs::read_to_string(path)?;
    
    // Skip Tree-sitter, go straight to Pest for scripts
    let ast = PestParser::parse(&source)?;
    
    // Type check / validate
    let typed_ast = type_check(ast)?;
    
    // Execute
    Interpreter::run(typed_ast)
}
```

## Testing Strategy

### Grammar Conformance Tests

```rust
#[test]
fn grammars_agree_on_valid_input() {
    let inputs = [
        "echo hello",
        "cat file | grep pattern",
        "out { key: value }",
        "agent \"prompt\"",
        // ...
    ];
    
    for input in inputs {
        let ts_result = ts_parse(input);
        let pest_result = pest_parse(input);
        
        // Both should succeed
        assert!(!ts_result.has_error());
        assert!(pest_result.is_ok());
        
        // AST structure should match
        assert_ast_equivalent(ts_result, pest_result.unwrap());
    }
}
```

### Error Recovery Tests

```rust
#[test]
fn tree_sitter_recovers_gracefully() {
    let broken_inputs = [
        "if x then",           // Missing fi
        "out { key: }",        // Missing value
        "agent",               // Missing prompt
    ];
    
    for input in broken_inputs {
        let ts_tree = ts_parse(input);
        
        // Should produce a tree (with ERROR nodes)
        assert!(ts_tree.root_node().child_count() > 0);
        
        // ERROR should be localized
        let errors: Vec<_> = find_error_nodes(&ts_tree).collect();
        assert!(errors.len() < 3, "Too many error nodes for: {}", input);
    }
}
```

## Future Directions

1. **Incremental Pest Parser**
   - Position-independent memoization
   - Rope-based source representation
   - Dependency tracking for invalidation

2. **Grammar Composition**
   - Allow agents to define new syntax
   - Dynamic grammar extension
   - Safe sandboxed parsing

3. **Bidirectional Sync**
   - Edit AST, regenerate source
   - Structural editing commands
   - Refactoring support

4. **Performance Optimization**
   - SIMD-accelerated tokenization
   - Parallel subtree parsing
   - Predictive parsing for REPL