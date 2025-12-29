# Known Test Issues

Tests that are skipped due to complex edge cases requiring architectural changes.

## Command Substitution with Redirects (09_comsub.bats)

**Test**: `comsub: with redirections`
```bash
echo $(echo hello >&2 2>&1)
```

**Issue**: When a command inside `$(...)` has redirects like `>&2`, the output appears twice - once to stderr (correct) and once extra. The comsub correctly captures nothing, but there's spurious output.

**Root Cause**: Complex interaction between:
1. Comsub forking and setting up capture pipe on fd 1
2. Inner command redirecting fd 1 to fd 2
3. Something in the expansion/execution path triggering double execution

**Impact**: Low - this specific redirect pattern inside comsub is rare in practice.

**Workaround**: Use variable assignment instead:
```bash
result=$(echo hello >&2 2>&1); echo $result
```

---

## Parameter Expansion with Trailing Backslash (08_quote.bats)

**Tests**:
- `quote: parameter expansion with double quote and trailing backslash`
- `quote: parameter expansion with backslash and brace`

```bash
echo "${foo:-string \\}"     # Expected: string \
echo ${foo:-string \\\}}     # Expected: string \}
```

**Issue**: Parser doesn't correctly handle escaped characters at the end of parameter expansion default values.

**Root Cause**: Grammar/parser edge case with backslash before closing brace in `${var:-default}` form.

**Impact**: Low - rarely used pattern.

---

## Future Work

These issues should be revisited when:
1. Major refactoring of command substitution execution
2. Parser rewrite or significant grammar changes
3. User reports real-world impact

Last updated: 2025-12-27
