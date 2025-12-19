# Heredoc Implementation Plan - F039

## Status
- **Tests Created**: 32 comprehensive tests in `tests/scripts/bash-compat/07_heredoc.bats`
- **Current Pass Rate**: 0/32 (0%)
- **Blocking Issue**: Heredoc content extraction works but is never injected into AST

## Problem Analysis

### Current Architecture

```
Source Code
    ↓
preprocess_heredocs()  ← WORKS: Extracts content to HashMap<usize, HeredocContent>
    ↓
Normalized Source + heredoc_contents map
    ↓
pest parser  ← WORKS: Creates Pairs tree with <<EOF markers
    ↓
ASTBuilder::build_redirect()  ← BUG: Creates RedirectTarget::File(word)
    ↓                               Should create: RedirectTarget::HereDoc{content, expand}
AST with Redirect nodes
    ↓
Executor::apply_redirects()  ← FAILS: Rejects File target with HereDoc kind
```

### Root Cause

**File**: `crates/colony-parser/src/builder.rs` (lines 1983-1986)

```rust
Rule::Word => {
    let word = Self::build_word(target_pair)?;
    RedirectTarget::File(word)  // ← BUG: Always creates File, even for heredocs
}
```

**Expected behavior**: When `kind` is `RedirectKind::HereDoc(...)`, should create:
```rust
RedirectTarget::HereDoc {
    content: String,  // From heredoc_contents map
    expand: bool,     // Based on quoted/unquoted delimiter
    kind: HereDocKind,
}
```

### Why This Happens

1. `preprocess_heredocs()` extracts heredoc content → `HashMap<usize, HeredocContent>`
2. Normalized source replaces heredoc content with placeholder
3. Parser sees `<<EOF` but no longer has access to the actual content
4. Builder receives `Word` containing delimiter, but has no heredoc content
5. Builder creates `RedirectTarget::File(delimiter_word)` 
6. Executor sees `RedirectKind::HereDoc` + `RedirectTarget::File` → error

### Evidence

**Error message**: `colony-exec: redirect failed: Invalid redirect: HereDoc should use apply_heredoc`

**Error location**: `crates/colony-exec/src/redirect.rs:355-359`
```rust
RedirectKind::HereDoc(_) => {
    return Err(ExecutionError::InvalidRedirect(
        "HereDoc should use apply_heredoc".to_string(),
    ))
}
```

This error is in `apply_file_redirect()`, which heredocs should never reach. They should go through the `RedirectTarget::HereDoc` match arm in `apply_redirects()` (line 191).

## Solution Design

### Phase 1: Thread heredoc_contents Through Builder

**Goal**: Make heredoc content available to `build_redirect()`

#### Changes to `pest_api.rs`

```rust
// Current
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let (normalized, _heredoc_contents) = preprocess_heredocs(source)?;
    //                 ^^^^^^^^^^^^^^^^^ Discarded!
    parse_direct(&normalized)
}

// Fixed
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let (normalized, heredoc_contents) = preprocess_heredocs(source)?;
    parse_with_heredoc_map(&normalized, heredoc_contents)
}

fn parse_with_heredoc_map(
    source: &str, 
    heredoc_map: HashMap<usize, HeredocContent>
) -> Result<Program, ParseError> {
    let pairs = ColonyParser::parse(Rule::Program, source)?;
    let pair = pairs.into_iter().next().unwrap();
    ASTBuilder::build_program_with_heredocs(pair, heredoc_map)  // New method
}
```

#### Changes to `builder.rs`

```rust
// Add field to ASTBuilder
pub struct ASTBuilder {
    heredoc_contents: HashMap<usize, HeredocContent>,
}

// Update build methods to accept heredoc map
impl ASTBuilder {
    pub fn build_program_with_heredocs(
        pair: Pair<Rule>, 
        heredoc_contents: HashMap<usize, HeredocContent>
    ) -> Result<Program, BuildError> {
        let builder = Self { heredoc_contents };
        builder.build_program_impl(pair)
    }
    
    // Pass self (with heredoc map) through all build methods
    fn build_statement(&self, pair: Pair<Rule>) -> Result<Statement, BuildError> {
        // ... can access self.heredoc_contents
    }
}
```

### Phase 2: Match Heredocs to Content

**Challenge**: How to match a heredoc in the AST to its content in the map?

**Option A: Position-based matching** (simpler)
- Key by line number + delimiter
- When building redirect with `<<EOF`, lookup by delimiter
- Works if delimiters are unique per line

**Option B: Index-based matching** (robust)
- Preprocessing assigns sequential indices
- Embed index in normalized source: `<<HEREDOC_0`
- Builder parses index from placeholder
- Most reliable but requires normalized source changes

**Recommendation**: Start with Option A (position-based), add Option B if needed.

### Phase 3: Inject Content in build_redirect()

```rust
fn build_redirect(&self, pair: Pair<Rule>) -> Result<Redirect, BuildError> {
    // ... parse fd, kind, target_pair ...
    
    let target = match (&kind, target_pair.as_rule()) {
        // Heredoc case
        (RedirectKind::HereDoc(heredoc_kind), Rule::Word) => {
            let delimiter_word = Self::build_word(target_pair)?;
            let delimiter = delimiter_word.as_literal()
                .ok_or(BuildError::InvalidHeredocDelimiter)?;
            
            // Lookup content by delimiter
            let heredoc_content = self.heredoc_contents.get(delimiter)
                .ok_or(BuildError::HeredocContentNotFound(delimiter.clone()))?;
            
            RedirectTarget::HereDoc {
                content: heredoc_content.content.clone(),
                expand: heredoc_content.expand,
                kind: *heredoc_kind,
            }
        }
        
        // Regular file redirect
        (_, Rule::Word) => {
            let word = Self::build_word(target_pair)?;
            RedirectTarget::File(word)
        }
        
        // Fd redirect
        (_, Rule::Integer) => {
            // ... existing code
        }
        
        _ => return Err(BuildError::InvalidRedirectTarget(...)),
    };
    
    Ok(Redirect { fd, kind, target, span })
}
```

### Phase 4: Handle Quoted Delimiters

Quoted delimiters (`<<'EOF'` or `<<"EOF"`) disable expansion.

**Current**: `HeredocContent` has `expand: bool` field
**Implementation**: `preprocess_heredocs()` already detects quotes and sets `expand` flag
**Status**: ✅ Already implemented, just need to pass through

## Implementation Steps

### Step 1: Minimal Wiring (2-3 tests passing)
1. Add `heredoc_contents: HashMap<String, HeredocContent>` to `ASTBuilder`
2. Thread map through `pest_api.rs::parse()` → `build_program_with_heredocs()`
3. In `build_redirect()`, detect `RedirectKind::HereDoc` + `Rule::Word`
4. Lookup content by delimiter literal, create `RedirectTarget::HereDoc`
5. Test: Basic heredoc with cat should work

**Expected result**: Tests 1-3 passing (basic heredocs)

### Step 2: Handle Edge Cases (5-10 tests passing)
1. Fix delimiter matching (handle line numbers for uniqueness)
2. Handle quoted delimiters (already extracted by preprocessing)
3. Handle empty heredocs
4. Test: Empty heredocs, quoted delimiters

**Expected result**: Tests 1-10 passing

### Step 3: Variable Expansion (15-20 tests passing)
1. In `apply_heredoc()`, implement actual variable expansion
2. Current code has TODO: `// TODO: Expand variables in heredoc content`
3. Use `Expander` to expand `$var` in content when `expand = true`
4. Test: Heredocs with variables

**Expected result**: Tests 1-20 passing

### Step 4: Advanced Features (25-30 tests passing)
1. Multiple heredocs on same line
2. Backslash-newline processing in unquoted heredocs
3. Heredocs in pipelines and command substitution
4. Tab stripping (`<<-`)

**Expected result**: Tests 1-30 passing

### Step 5: Edge Cases (32/32 = 100%)
1. Very long delimiters
2. Delimiter-like content
3. Multiple blank lines
4. All comparison tests with bash

**Expected result**: 32/32 tests passing (100%)

## Files to Modify

### Primary Changes
1. **`crates/colony-parser/src/pest_api.rs`** (~20 lines)
   - Pass heredoc_contents to builder
   - Add `parse_with_heredoc_map()` helper

2. **`crates/colony-parser/src/builder.rs`** (~50 lines)
   - Add heredoc_contents field to ASTBuilder
   - Update `build_redirect()` to create HereDoc targets
   - Add error variants for heredoc issues

3. **`crates/colony-exec/src/redirect.rs`** (~10 lines)
   - Implement variable expansion in `apply_heredoc()`
   - Currently just returns literal content

### Supporting Changes
4. **`crates/colony-parser/src/heredoc.rs`** (minimal)
   - Already complete, may need delimiter extraction helper

5. **`crates/colony-parser/src/ast.rs`** (check)
   - Verify `RedirectTarget::HereDoc` has correct fields

## Test Coverage

### Test Categories (32 total)

| Category | Tests | Priority |
|----------|-------|----------|
| Basic heredocs | 1-4 | P0 (Core) |
| Empty heredocs | 5-7 | P1 |
| Tab stripping | 8-9 | P1 |
| Multiple heredocs | 10-11 | P2 |
| Quoted delimiters | 12-13 | P0 (Core) |
| Backslash-newline | 14-17 | P2 |
| File operations | 18-19 | P1 |
| Special characters | 20-21 | P2 |
| Pipelines/comsub | 22-23 | P1 |
| Edge cases | 24-28 | P2 |
| Bash comparison | 29-32 | P1 (Validation) |

**Target**: Start with P0 tests (1-4, 12-13) = 6 tests
**Goal**: 100% (32/32)

## Known Challenges

### Challenge 1: Delimiter Uniqueness
**Problem**: Multiple heredocs with same delimiter on different lines
**Solution**: Key by `(line_number, delimiter)` instead of just `delimiter`

### Challenge 2: Multiple Heredocs Per Line
**Problem**: `cat <<EOF1 <<EOF2`
**Solution**: Index-based approach with placeholder replacement
**Status**: Currently ignored (test marked `#[ignore]`)

### Challenge 3: Variable Expansion Scope
**Problem**: Heredoc expansion needs access to shell variables
**Solution**: `apply_heredoc()` already has `ExpansionContext` parameter
**Status**: Just need to call `Expander::expand_word()` on content

### Challenge 4: Backslash-Newline
**Problem**: Unquoted heredocs process `\<newline>` → continuation
**Solution**: Preprocess content before writing to pipe
**Status**: New logic needed in `apply_heredoc()`

## Success Criteria

- [ ] 32/32 tests passing (100%)
- [ ] All heredoc types supported: `<<`, `<<-`, `<<<`
- [ ] Quoted and unquoted delimiters work correctly
- [ ] Variable expansion in unquoted heredocs
- [ ] No expansion in quoted heredocs
- [ ] Tab stripping with `<<-`
- [ ] Multiple heredocs per command
- [ ] Heredocs in pipelines and command substitution
- [ ] Exact bash compatibility (comparison tests pass)

## Next Session Action Items

1. **START HERE**: Run existing tests to confirm 0/32 baseline
2. Implement Step 1 (Minimal Wiring) - target 2-3 passing tests
3. Add debug logging to see heredoc_contents at each stage
4. Test simple heredoc: `cat <<EOF\ntest\nEOF`
5. Verify `RedirectTarget::HereDoc` is created correctly
6. Continue through implementation steps sequentially

## References

- Test file: `tests/scripts/bash-compat/07_heredoc.bats`
- Bash reference: `/Users/mcfearsome/src/github.com/bminor/bash/tests/heredoc.tests`
- TODO marker: `crates/colony-parser/src/pest_api.rs:127`
- Preprocessing: `crates/colony-parser/src/heredoc.rs`
- Redirect application: `crates/colony-exec/src/redirect.rs:191-195`

## Estimated Effort

- **Step 1 (Wiring)**: 1-2 hours
- **Step 2 (Edge Cases)**: 1 hour  
- **Step 3 (Expansion)**: 1 hour
- **Step 4 (Advanced)**: 2 hours
- **Step 5 (Polish)**: 1 hour

**Total**: 6-8 hours for 100% heredoc support

---

**Document Created**: 2025-12-19 23:30
**Session**: Completed F038 (Case 100%), Started F039 (Heredoc 0%)
**Next**: Resume F039 heredoc implementation following this plan
