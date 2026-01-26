# Colony-Shell POSIX Compliance Audit

**Date:** 2025-01-26  
**Audited Against:** POSIX.1-2017 (IEEE Std 1003.1-2017) Shell Command Language

## Executive Summary

Colony-shell is approximately **95% POSIX compliant** for typical shell scripts. The implementation includes all POSIX special builtins, most regular builtins, complete expansion support, and full control flow structures. The main gaps are a few missing builtins (`command`, `kill`, `umask`, `ulimit`) and incomplete job control (`fg`/`bg`).

---

## 1. Crate Architecture

### Core Shell Crates

| Crate | Purpose | Lines (approx) |
|-------|---------|----------------|
| `colony-parser` | PEG-based shell syntax parser (POSIX/Bash compatible) | ~3000 |
| `colony-expand` | Word expansion (variables, globs, tilde, arithmetic, command sub) | ~4000 |
| `colony-exec` | Command execution, pipelines, job control, builtins, redirects | ~6000 |
| `colony-shell` | Main shell binary, REPL, control flow orchestration, traps | ~3000 |

### Supporting Crates

| Crate | Purpose |
|-------|---------|
| `colony-channels` | Extended channel manager (fd 3-6: stdcontext, stdllm, stdtool, stdtrace) |
| `colony-tuple` | Tuple space for reactive environment (IPC between shell components) |
| `colony-config` | Configuration discovery and trust management |
| `colony-agent` | LLM agent integration (Anthropic, OpenAI, Ollama providers) |
| `colony-mcp` | Model Context Protocol client |
| `colony-protocol` | IPC protocol definitions (OSC 2337) |
| `colony-memory` | Vector embeddings and storage (LanceDB/fastembed) |
| `colony-secure` | Keychain, encryption, approval registry for secure operations |
| `colony-filetree` | File tree state, watching, git status tracking |
| `colony-tree-sitter` | Tree-sitter shell grammar integration |
| `colony-tui` | Terminal UI (prompt, input, highlighting, history search) |
| `colony-ui-core` | UI components, widgets, sidebar panels |
| `colony-focus` | Focus tracking |

---

## 2. Parser Capabilities (colony-parser)

### Statement Types - All Implemented ✅

| Type | Syntax | Status |
|------|--------|--------|
| SimpleCommand | `cmd arg1 arg2` | ✅ Working |
| Pipeline | `cmd1 \| cmd2 \| cmd3` | ✅ Working |
| CommandList | `cmd1 && cmd2 \|\| cmd3` | ✅ Working |
| BraceGroup | `{ commands; }` | ✅ Working |
| Subshell | `( commands )` | ✅ Working |
| If/Elif/Else | `if/then/elif/else/fi` | ✅ Working |
| Case | `case/esac` with `;;`, `;&`, `;;&` | ✅ Working |
| For | `for var in words; do..done` | ✅ Working |
| CStyleFor | `for ((init; cond; update))` | ✅ Working |
| While | `while cond; do..done` | ✅ Working |
| Until | `until cond; do..done` | ✅ Working |
| Conditional | `[[ expression ]]` | ✅ Working |
| Arithmetic | `(( expression ))` | ✅ Working |
| Coproc | `coproc [NAME] command` | ✅ Working |
| FunctionDef | `name() { }` and `function name { }` | ✅ Working |

### Word Parts - All Implemented ✅

| Type | Syntax | Status |
|------|--------|--------|
| Literal | plain text | ✅ |
| SingleQuoted | `'text'` | ✅ |
| DoubleQuoted | `"text with $var"` | ✅ |
| AnsiCQuoted | `$'text with \n'` | ✅ |
| Variable | `$var`, `${var}`, all modifiers | ✅ |
| CommandSub | `$(cmd)` and backticks | ✅ |
| ArithRaw | `$((expr))` | ✅ |
| Tilde | `~` and `~user` | ✅ |
| BraceExpansion | `{a,b,c}` and `{1..5}` | ✅ |
| ProcessSub | `<(cmd)` and `>(cmd)` | ✅ |
| ExtGlob | `?(pat)`, `*(pat)`, `+(pat)`, `@(pat)`, `!(pat)` | ✅ |

### Redirections - All Implemented ✅

| Type | Syntax | Status |
|------|--------|--------|
| Input | `<`, `<>` | ✅ |
| Output | `>`, `>>`, `>\|` | ✅ |
| FD duplication | `>&N`, `<&N` | ✅ |
| FD closing | `>&-`, `<&-` | ✅ |
| Here-doc | `<<`, `<<-` | ✅ |
| Here-string | `<<<` | ✅ |
| Combined | `&>`, `&>>` | ✅ |
| Variable fd | `{fd}>file` | ✅ |

### Variable Modifiers - All Implemented ✅

| Category | Modifiers | Status |
|----------|-----------|--------|
| Default | `${var:-word}`, `${var-word}` | ✅ |
| Assign | `${var:=word}`, `${var=word}` | ✅ |
| Error | `${var:?word}`, `${var?word}` | ✅ |
| Alternate | `${var:+word}`, `${var+word}` | ✅ |
| Length | `${#var}`, `${#arr[@]}` | ✅ |
| Indices | `${!arr[@]}` | ✅ |
| Slice | `${var:offset:length}` | ✅ |
| Strip prefix | `${var#pat}`, `${var##pat}` | ✅ |
| Strip suffix | `${var%pat}`, `${var%%pat}` | ✅ |
| Case | `${var^}`, `${var^^}`, `${var,}`, `${var,,}` | ✅ |
| Substitute | `${var/pat/repl}`, `${var//pat/repl}` | ✅ |

---

## 3. Execution Capabilities (colony-exec)

### Pipeline Features ✅

- Multi-command pipelines with concurrent execution
- `|` - standard pipe (stdout to stdin)
- `|&` - pipe stderr too
- `!` prefix for negation
- `lastpipe` shopt for running last command in current shell
- `pipefail` option support

### Command Substitution ✅

- `$(command)` - POSIX style
- Nested command substitution
- Exit status propagation from last command

### Process Substitution ✅

- `<(command)` - input process substitution
- `>(command)` - output process substitution
- Implemented via /dev/fd/N

---

## 4. Builtin Commands

### POSIX Special Builtins - All Implemented ✅

| Builtin | Status | Implementation Notes |
|---------|--------|---------------------|
| `.` (source) | ✅ Working | Full POSIX semantics including positional params |
| `:` | ✅ Working | Null utility returns success |
| `break` | ✅ Working | Via ControlFlow::Break |
| `continue` | ✅ Working | Via ControlFlow::Continue |
| `eval` | ✅ Working | BuiltinResult::Eval variant |
| `exec` | ✅ Working | With redirect support |
| `exit` | ✅ Working | BuiltinResult::Exit with code |
| `export` | ✅ Working | Includes -f, -n, -p |
| `readonly` | ✅ Working | VarAttributes.readonly tracking |
| `return` | ✅ Working | Functions and sourced files |
| `set` | ✅ Working | -e, -u, -x, -f, -n, -v, -o, -- |
| `shift` | ✅ Working | builtin_shift() |
| `trap` | ✅ Working | EXIT, ERR, DEBUG, RETURN, signals |
| `unset` | ✅ Working | -f for functions, -n for namerefs |

### POSIX Regular Builtins

| Builtin | Status | Notes |
|---------|--------|-------|
| `alias` | ✅ Working | Full expansion with trailing space |
| `bg` | ⚠️ Stub | "would background job (not yet implemented)" |
| `cd` | ✅ Working | HOME, OLDPWD, `-` handling |
| `command` | ❌ **MISSING** | Listed but no handler |
| `fc` | ❌ **MISSING** | Listed but no handler |
| `fg` | ⚠️ Stub | "would foreground job (not yet implemented)" |
| `getopts` | ✅ Working | builtin_getopts() |
| `hash` | ⚠️ Stub | Returns success, no path caching |
| `jobs` | ✅ Working | Lists job table |
| `kill` | ❌ **MISSING** | Listed but no handler |
| `pwd` | ✅ Working | |
| `read` | ✅ Working | IFS, -r, -a, -n, -d, -u, -t |
| `type` | ✅ Working | -a, -f, -p, -P, -t |
| `ulimit` | ❌ **MISSING** | Listed but no handler |
| `umask` | ❌ **MISSING** | Listed but no handler |
| `unalias` | ✅ Working | -a for all |
| `wait` | ✅ Working | PID and job ID support |

### Bash Extension Builtins - Implemented ✅

| Builtin | Status | Notes |
|---------|--------|-------|
| `declare` | ✅ Working | -a, -A, -i, -l, -u, -r, -x, -n, -p |
| `local` | ✅ Working | |
| `typeset` | ✅ Working | Alias for declare |
| `shopt` | ✅ Working | 24+ options |
| `printf` | ✅ Working | Standard format specifiers |
| `mapfile`/`readarray` | ✅ Working | |
| `history` | ✅ Working | |
| `complete` | ✅ Working | Programmable completion |
| `compgen` | ✅ Working | |
| `dirs` | ✅ Working | Directory stack |
| `pushd` | ✅ Working | |
| `popd` | ✅ Working | |
| `disown` | ✅ Working | -a, -r, -h |

---

## 5. Expansion Features (colony-expand)

### All Expansion Types Implemented ✅

| Expansion | Status | Details |
|-----------|--------|---------|
| Tilde | ✅ | `~`, `~user`, `~+`, `~-` |
| Parameter | ✅ | All POSIX + Bash modifiers |
| Brace | ✅ | `{a,b}`, `{1..5}`, `{1..10..2}` |
| Command Sub | ✅ | `$(cmd)`, nested |
| Arithmetic | ✅ | Full C-style with all operators |
| Pathname (Glob) | ✅ | `*`, `?`, `[...]`, extglob, globstar |
| Field Splitting | ✅ | IFS-based, `$@` vs `$*` handling |
| Quote Removal | ✅ | Single, double, ANSI-C |
| Process Sub | ✅ | `<(cmd)`, `>(cmd)` |

### Special Parameters ✅

| Parameter | Status |
|-----------|--------|
| `$@`, `$*`, `$#` | ✅ Positional |
| `$?` | ✅ Exit status |
| `$$` | ✅ Shell PID |
| `$!` | ✅ Background PID |
| `$0` | ✅ Script name |
| `$-` | ✅ Option flags |
| `$RANDOM`, `$SRANDOM` | ✅ Random numbers |
| `$SECONDS` | ✅ Elapsed time |
| `$LINENO` | ✅ Line number |
| `$BASHPID` | ✅ Current PID |
| `$EPOCHSECONDS`, `$EPOCHREALTIME` | ✅ Timestamps |

---

## 6. Job Control

### Implemented ✅

- Job table with process groups
- Job ID tracking (1-based)
- Job status: Running, Background, Stopped, Done
- `jobs` builtin to list
- `wait` builtin (PIDs and job specs)
- `disown` builtin
- Background execution with `&`
- Signal setup (SIGINT, SIGTSTP, SIGTTOU ignored in shell)

### Incomplete ⚠️

- `fg` - Stub only, needs SIGCONT + tcsetpgrp
- `bg` - Stub only, needs SIGCONT
- Runtime SIGCHLD handler for async job completion

---

## 7. Control Flow

### All Control Structures Implemented ✅

| Structure | Status | Notes |
|-----------|--------|-------|
| if/elif/else | ✅ | Command-based conditions, errexit handling |
| for | ✅ | Traditional and C-style |
| while | ✅ | With break/continue |
| until | ✅ | With break/continue |
| case | ✅ | `;;`, `;&`, `;;&` terminators |
| functions | ✅ | Local vars, export -f, recursion |

### Control Flow Tracking ✅

- `ControlFlow` enum: Normal, Break, Continue, Exit, Return
- Proper propagation through nested structures
- Loop depth tracking for break N / continue N

---

## 8. Special Features

### Traps ✅

| Trap | Status | Notes |
|------|--------|-------|
| EXIT | ✅ | Runs on shell/subshell exit |
| ERR | ✅ | Runs on command failure |
| DEBUG | ✅ | Runs before each command |
| RETURN | ✅ | Runs on function/source return |
| Signals | ✅ | HUP, INT, QUIT, TERM, etc. |

### Functions ✅

- POSIX syntax: `name() { body }`
- Bash syntax: `function name { body }`
- Local variables via `local` or `declare`
- Function export (`export -f`)
- Recursive function calls

### Aliases ✅

- Definition: `alias name='value'`
- Expansion with cycle detection
- Trailing space expansion
- Complex alias parsing (reparsing if needed)
- `expand_aliases` shopt

### Shell Options ✅

**set options:** errexit (-e), nounset (-u), xtrace (-x), noglob (-f), noexec (-n), verbose (-v), noclobber (-C), pipefail

**shopt options (24+):** extglob, globstar, nullglob, dotglob, failglob, nocaseglob, nocasematch, lastpipe, expand_aliases, sourcepath, checkwinsize, histexpand, progcomp, and more

### Conditional Command `[[ ]]` ✅

- String tests: `-n`, `-z`
- File tests: `-e`, `-f`, `-d`, `-r`, `-w`, `-x`, `-s`, `-L`, `-h`, `-p`, `-S`, `-b`, `-c`, `-t`, `-O`, `-G`, `-N`
- Comparison: `==`, `!=`, `<`, `>`, `-eq`, `-ne`, `-lt`, `-le`, `-gt`, `-ge`
- Regex match: `=~`
- File comparison: `-nt`, `-ot`, `-ef`
- Logic: `&&`, `||`, `!`, `()`

### Arithmetic Command `(( ))` ✅

- Full C-style arithmetic
- All operators including assignment
- Returns 0 if result non-zero, 1 if zero

---

## 9. Identified Gaps

### Critical - Missing POSIX Builtins

| Builtin | Priority | Effort | Impact |
|---------|----------|--------|--------|
| `command` | **HIGH** | Small | Scripts use `command -v` frequently |
| `kill` | **HIGH** | Medium | Signal handling in scripts |
| `umask` | Medium | Small | File creation mask |
| `ulimit` | Medium | Medium | Resource limits |
| `fc` | Low | Large | Interactive history editing |

### Non-POSIX Behaviors

| Issue | Priority | Notes |
|-------|----------|-------|
| Command search order | Medium | Special builtins should precede functions |
| Special builtin errors | Medium | Should be fatal in non-interactive shell |
| Signal handlers | Medium | Need runtime SIGINT/SIGCHLD handling |

### Incomplete Features

| Feature | Priority | Status |
|---------|----------|--------|
| `fg` | Medium | Stub - needs SIGCONT, tcsetpgrp |
| `bg` | Medium | Stub - needs SIGCONT |
| `hash` | Low | Stub - no actual path caching |

---

## 10. Colony-Specific Extensions (Non-POSIX)

These are intentional extensions, not compliance gaps:

| Feature | Status | Purpose |
|---------|--------|---------|
| Extended channels (fd 3-6) | ✅ | stdcontext, stdllm, stdtool, stdtrace |
| Tuple space IPC | ✅ | Reactive environment updates |
| `var` declarations | ✅ | Colony typed variables |
| `proc` definitions | ✅ | Colony typed procedures |
| Typed pipes (`->`, `=>`, `~>`) | ✅ | JSON, context, agent pipes |
| Agent integration | ✅ | LLM providers, MCP |
| OSC 2337 protocol | ✅ | Terminal communication |

---

## 11. Recommended Implementation Roadmap

### Phase 1 - Core POSIX Compliance (1-2 days)

```
├── command builtin
│   ├── -v (print command path)
│   ├── -V (verbose description)
│   └── -p (use default PATH)
│
├── kill builtin
│   ├── -l (list signals)
│   ├── -s SIG (specify signal)
│   └── PID/job-spec targets
│
└── umask builtin
    ├── Print current mask
    └── Set new mask (octal or symbolic)
```

### Phase 2 - Complete Job Control (2-3 days)

```
├── fg builtin
│   ├── SIGCONT to process group
│   ├── tcsetpgrp for terminal control
│   └── Wait for foreground completion
│
├── bg builtin
│   ├── SIGCONT to process group
│   └── Update job status
│
└── SIGCHLD handler
    └── Async job status updates
```

### Phase 3 - Polish (1-2 days)

```
├── ulimit builtin
│   ├── getrlimit/setrlimit
│   └── All resource types
│
├── Special builtin distinction
│   ├── Search order fix
│   └── Error handling fix
│
├── hash builtin
│   └── PATH cache management
│
└── fc builtin (optional)
    └── History editing
```

---

## 12. Test Coverage

### Existing Tests

- **colony-parser:** 89+ tests (grammar, AST)
- **colony-expand:** 100+ tests (expansions)
- **colony-exec:** 50+ tests (execution, builtins)
- **BATS integration:** 200+ tests across features

### Recommended Additional Tests

- `command` builtin: 10 tests (bypass functions, -v, -V, -p)
- `kill` builtin: 15 tests (signals, job specs, error cases)
- `umask` builtin: 8 tests (get, set octal, set symbolic)
- Job control: 20 tests (fg, bg, signal handling)

---

## Conclusion

Colony-shell has a **mature, comprehensive shell implementation** that handles the vast majority of real-world shell scripts correctly. The remaining gaps are well-defined and can be addressed in approximately 1 week of focused development.

**Strengths:**
- Complete parser with all POSIX + Bash syntax
- Full expansion support including process substitution
- All control flow structures
- Comprehensive trap system
- 45+ builtins implemented

**Priority Gaps:**
1. `command` builtin (most impactful)
2. `kill` builtin (important for scripts)
3. Full job control (fg/bg with signals)
