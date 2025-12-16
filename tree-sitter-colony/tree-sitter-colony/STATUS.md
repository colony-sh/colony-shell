# Tree-sitter Colony Status

## Current State

**Status:** Grammar in development, not yet functional

**Problem:** The Colony grammar (from Downloads/grammar.js) has unresolved conflicts that prevent generation:
- `channel_number` vs `file_descriptor` (both match /[0-9]+/)
- `variable_assignment` vs `command` (ambiguity with @ symbol)
- `function_definition` vs `_literal` (word followed by {})

**Attempted Solutions:**
1. Started with minimal POSIX grammar → word tokenization issues
2. Tried comprehensive Colony grammar → multiple conflicts

## Recommended Approach

### Fork tree-sitter-bash

**Rationale:**
- tree-sitter-bash is mature, battle-tested (used by GitHub, many editors)
- Handles shell tokenization edge cases correctly
- Has proper word boundary detection
- Includes scanner.c for context-sensitive parsing (heredocs, concat)

**Plan:**
1. Clone tree-sitter-bash
2. Copy to tree-sitter-colony
3. Extend gradually with Colony features:
   - Start with POSIX baseline (working)
   - Add typed pipes (->)
   - Add buffer references ($[n])
   - Add tuple operations (out/in/rd)
   - Add agent commands
   - Add capability blocks

**Benefits:**
- Start with working grammar
- Incremental addition of features
- Test each feature before adding next
- Maintain POSIX compatibility

### Alternative: Focus on Pest/Hand-written

**Current Status:**
- F001/F002 (hand-written) - Complete, 28 tests passing
- F021 (Pest) - In progress, 37 tests passing
- Both parsers work for execution

**Recommendation:**
- Use Pest/hand-written for execution (already working)
- Add Tree-sitter later when we have time to properly fork tree-sitter-bash
- Tree-sitter is "nice to have" for editor features, not critical for shell functionality

## Next Steps

### Option A: Fork tree-sitter-bash Now
```bash
# 1. Clone tree-sitter-bash
git clone https://github.com/tree-sitter/tree-sitter-bash /tmp/ts-bash

# 2. Copy files
cp -r /tmp/ts-bash/* tree-sitter-colony/
mv tree-sitter-colony/grammar.js tree-sitter-colony/grammar-bash-base.js

# 3. Extend grammar incrementally
# Start with bash baseline, add Colony features one by one

# 4. Test after each addition
npx tree-sitter test
```

### Option B: Defer Tree-sitter
```bash
# Mark F022 as blocked, continue with other features
# Tree-sitter is not blocking shell execution

# Focus on:
# - F006 (Redirections)
# - F007 (Core builtins)
# - F021 (Complete Pest parser)

# Return to Tree-sitter when we have bandwidth
```

## Files Created

```
tree-sitter-colony/
├── grammar.js              # Colony grammar (has conflicts)
├── grammar.js.old          # Earlier version
├── bindings/rust/          # Rust bindings (ready for parser.rs)
├── queries/highlights.scm  # Highlighting queries
├── test/corpus/            # Test cases
├── examples/test.sh        # Example script
├── README.md               # Documentation
├── QUICKSTART.md
├── INTEGRATION.md
├── RECONCILIATION.md
├── RUST_SITTER.md
└── STATUS.md               # This file
```

## Decision Point

**Recommend:** Option B (defer Tree-sitter)

**Why:**
- Core shell execution is working (F003-F005 complete)
- Pest parser progressing well (F021)
- Tree-sitter grammar needs significant debugging
- Not blocking any other features
- Can be perfected later with tree-sitter-bash fork

**Time saved** can be invested in:
- Completing F021 (Pest parser)
- Implementing F006 (Redirections)
- Implementing F007 (Core builtins)
- Real shell functionality

Tree-sitter is valuable for editor features, but the shell works fine without it for now.
