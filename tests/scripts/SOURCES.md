# Test Sources and Attribution

## Official Bash Test Suite

**Source**: https://github.com/bminor/bash/tree/master/tests
**License**: GNU General Public License v3.0 or later
**Copyright**: Free Software Foundation, Inc.
**Date Sourced**: 2025-12-17

### Usage Rights

The bash test suite is GPL-licensed, but:
- We can **run** these tests against Colony (GPL allows testing compatibility)
- We **reference** the original tests and results
- We **do not redistribute** the bash test code itself
- Colony users should download bash tests directly from GNU if needed

### Test Selection Strategy

We focus on POSIX-compliant features that Colony currently supports:

#### Phase 1: Core Features (Implemented)
- ✅ `builtins.tests` - Core builtins (cd, export, echo, pwd, etc.)
- ✅ `varenv.tests` - Variable expansion and environment
- ✅ `quote.tests` - Quoting rules (single, double, escape)
- ✅ `redir.tests` - Redirections (<, >, >>, heredocs)
- ✅ `pipeline.tests` - Pipeline execution

#### Phase 2: Control Flow (Implemented)
- ✅ `case.tests` - Case statements
- ✅ `conditional.tests` - If/then/else/fi
- ✅ `loop.tests` - For/while/until loops
- ✅ `func.tests` - Function definitions

#### Phase 3: Advanced (Partially Implemented)
- ⏳ `arith.tests` - Arithmetic expansion (parser ready, evaluator TODO)
- ⏳ `glob.tests` - Pathname expansion (detection ready, expansion TODO)
- ⏳ `source.tests` - Source/dot command (stub exists)
- ❌ `array.tests` - Arrays (not in POSIX, skip)
- ❌ `history.tests` - History (not implemented yet)
- ❌ `jobs.tests` - Job control (basic infrastructure only)

#### Phase 4: Colony Enhancements
These are **not** from bash test suite - created by us:
- 🔵 `typed-pipes.tests` - Colony typed pipes (->. =>)
- 🔵 `reactive-tuples.tests` - Tuple space integration
- 🔵 `modern-syntax.tests` - var/const/setvar

### Attribution Format

Each test directory includes:

```
tests/scripts/bash-compat/01_builtins/
├── builtins.tests          # Original bash test (symlink or copy)
├── builtins.right          # Expected output from bash
├── LICENSE                 # GPL v3 license text
└── SOURCE.txt              # URL and git commit hash
```

## Additional Sources

### Educational Scripts

**Source**: Various educational bash tutorials and examples
**License**: MIT, BSD, Apache 2.0, or Public Domain only
**Criteria**: Well-commented, teaching-focused, permissively licensed

We maintain attribution for each script in:
- `metadata.toml` with `source.url` and `source.license`
- Links back to original source

### Community Contributions

Scripts contributed to this project:
- **License**: Must be MIT or Apache 2.0
- **Attribution**: Maintained in git history and metadata.toml

---

## License Compliance

- ✅ GPL tests: Run only, do not redistribute, reference results
- ✅ MIT/BSD/Apache: Can adapt and redistribute with attribution
- ❌ No license: Do not include
- ❌ Proprietary: Do not include

## Verification

Before adding any script:
1. Verify license allows our usage
2. Record source URL and commit/date
3. Include LICENSE file in test directory
4. Update this file with attribution
