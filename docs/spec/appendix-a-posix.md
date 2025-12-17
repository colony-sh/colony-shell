# Appendix A: POSIX Compatibility

## A.1 Overview

Colony Shell aims for full POSIX.1-2017 Shell Command Language compliance when running in POSIX mode (`colony-sh` binary or `--posix` flag). This appendix documents supported features, intentional deviations, and compatibility notes.

## A.2 Supported POSIX Features

### A.2.1 Lexical Structure

| Feature | Status | Notes |
|---------|--------|-------|
| Simple commands | ✅ Complete | |
| Pipelines | ✅ Complete | Both `|` and `|&` |
| Lists (`;`, `&`, `&&`, `||`) | ✅ Complete | |
| Compound commands | ✅ Complete | if/while/until/for/case |
| Function definitions | ✅ Complete | Both `name()` and `function name` |
| Command substitution `$(...)` | ✅ Complete | Recursive nesting supported |
| Command substitution `` `...` `` | ✅ Complete | Legacy backtick syntax |
| Arithmetic expansion `$((...))` | ✅ Complete | |
| Parameter expansion | ✅ Complete | All `${...}` forms |
| Single quotes | ✅ Complete | Literal preservation |
| Double quotes | ✅ Complete | With expansion |
| Escape sequences | ✅ Complete | `\` escaping |
| Here-documents | ✅ Complete | `<<` and `<<-` |
| Here-strings | ✅ Complete | `<<<` |
| Comments | ✅ Complete | `#` to end of line |

### A.2.2 Redirections

| Feature | Status | Notes |
|---------|--------|-------|
| `>` (output) | ✅ Complete | |
| `>>` (append) | ✅ Complete | |
| `<` (input) | ✅ Complete | |
| `<>` (read-write) | ✅ Complete | |
| `>|` (clobber) | ✅ Complete | |
| `N>` (fd output) | ✅ Complete | |
| `N<` (fd input) | ✅ Complete | |
| `>&N` (dup output) | ✅ Complete | |
| `<&N` (dup input) | ✅ Complete | |
| `>&-` (close output) | ✅ Complete | |
| `<&-` (close input) | ✅ Complete | |
| `<<` (here-doc) | ✅ Complete | |
| `<<-` (here-doc, strip tabs) | ✅ Complete | |
| `<<<` (here-string) | ✅ Complete | |
| `/dev/fd/N` | ✅ Complete | Via OS support |
| Process substitution `<(...)` | ✅ Complete | |
| Process substitution `>(...)` | ✅ Complete | |

### A.2.3 Parameter Expansion

| Form | Status | Description |
|------|--------|-------------|
| `$var` | ✅ | Simple expansion |
| `${var}` | ✅ | Braced expansion |
| `${var:-word}` | ✅ | Default if unset or null |
| `${var-word}` | ✅ | Default if unset |
| `${var:=word}` | ✅ | Assign default if unset or null |
| `${var=word}` | ✅ | Assign default if unset |
| `${var:?word}` | ✅ | Error if unset or null |
| `${var?word}` | ✅ | Error if unset |
| `${var:+word}` | ✅ | Alternate if set and non-null |
| `${var+word}` | ✅ | Alternate if set |
| `${#var}` | ✅ | String length |
| `${var%pattern}` | ✅ | Remove shortest suffix |
| `${var%%pattern}` | ✅ | Remove longest suffix |
| `${var#pattern}` | ✅ | Remove shortest prefix |
| `${var##pattern}` | ✅ | Remove longest prefix |
| `${var:offset}` | ✅ | Substring from offset |
| `${var:offset:length}` | ✅ | Substring with length |
| `${var/pat/repl}` | ✅ | Replace first match |
| `${var//pat/repl}` | ✅ | Replace all matches |
| `${var/#pat/repl}` | ✅ | Replace prefix |
| `${var/%pat/repl}` | ✅ | Replace suffix |
| `${!prefix*}` | ✅ | Variable names with prefix |
| `${!name[@]}` | ✅ | Array indices |
| `${var^}` | ✅ | Uppercase first char |
| `${var^^}` | ✅ | Uppercase all |
| `${var,}` | ✅ | Lowercase first char |
| `${var,,}` | ✅ | Lowercase all |

### A.2.4 Special Parameters

| Parameter | Status | Description |
|-----------|--------|-------------|
| `$0` | ✅ | Shell or script name |
| `$1`-`$9` | ✅ | Positional parameters |
| `${10}` etc | ✅ | Extended positional |
| `$#` | ✅ | Number of positional params |
| `$*` | ✅ | All positional (single word) |
| `$@` | ✅ | All positional (separate words) |
| `$?` | ✅ | Exit status of last command |
| `$-` | ✅ | Current option flags |
| `$$` | ✅ | Process ID of shell |
| `$!` | ✅ | PID of last background job |
| `$_` | ✅ | Last argument of prev command |

### A.2.5 Builtins

| Builtin | Status | Notes |
|---------|--------|-------|
| `:` (colon) | ✅ | Null command |
| `.` (dot) | 🚧 | Stubbed |
| `alias` | 🚧 | Stubbed |
| `bg` | 🚧 | In progress (F008) |
| `break` | ✅ | |
| `cd` | ✅ | |
| `command` | 🚧 | |
| `continue` | ✅ | |
| `echo` | ✅ | |
| `eval` | 🚧 | |
| `exec` | 🚧 | |
| `exit` | ✅ | |
| `export` | ✅ | |
| `false` | ✅ | |
| `fg` | 🚧 | In progress (F008) |
| `getopts` | 🚧 | |
| `hash` | 🚧 | |
| `jobs` | 🚧 | In progress (F008) |
| `kill` | 🚧 | |
| `pwd` | ✅ | |
| `read` | 🚧 | |
| `readonly` | 🚧 | |
| `return` | ✅ | |
| `set` | 🚧 | Partial |
| `shift` | ✅ | |
| `source` | 🚧 | Stubbed |
| `test` / `[` | ✅ | |
| `times` | 🚧 | |
| `trap` | 🚧 | |
| `true` | ✅ | |
| `type` | ✅ | |
| `ulimit` | 🚧 | |
| `umask` | 🚧 | |
| `unalias` | 🚧 | Stubbed |
| `unset` | ✅ | |
| `wait` | 🚧 | In progress (F008) |

**Legend:** ✅ Complete | 🚧 Partial/In Progress | ❌ Not Implemented

### A.2.6 Options (set -o)

| Option | Status | Description |
|--------|--------|-------------|
| `errexit` (-e) | ✅ | Exit on error |
| `nounset` (-u) | ✅ | Error on unset variable |
| `xtrace` (-x) | 🚧 | Print commands before execution |
| `verbose` (-v) | 🚧 | Print input lines |
| `noclobber` (-C) | ✅ | Don't overwrite existing files |
| `noglob` (-f) | ✅ | Disable pathname expansion |
| `allexport` (-a) | 🚧 | Export all variables |
| `pipefail` | ✅ | Pipeline fails on any command |
| `monitor` (-m) | 🚧 | Job control |
| `notify` (-b) | 🚧 | Immediate job notification |

## A.3 Intentional Deviations

Colony makes the following intentional deviations from POSIX when in **Modern mode** (`colony` binary):

### A.3.1 Safer Defaults

| Feature | POSIX Behavior | Colony Modern Behavior |
|---------|---------------|------------------------|
| Unset variables | Empty string | Error (like `set -u`) |
| Command failure | Continue | Exit (like `set -e`) |
| Pipeline failure | Last exit code | Any failure (pipefail) |
| Word splitting | On by default | Disabled by default |
| Glob no match | Literal pattern | Error |

### A.3.2 Enhanced Syntax

Colony Modern mode adds syntax not in POSIX:

| Feature | Example | Notes |
|---------|---------|-------|
| Typed pipes | `cmd -> jq` | Type annotation flow |
| Fat arrow | `cmd => target` | Auto-converting pipe |
| Alter pipe | `cmd ~> edit` | Structured modification |
| Type annotations | `var x: Int = 42` | Optional types |
| Triple quotes | `"""multiline"""` | Multi-line strings |
| Raw strings | `r'path\to\file'` | No escape processing |
| Lambda | `|x| x + 1` | Inline functions |
| Match expression | `match $x { ... }` | Pattern matching |

### A.3.3 Reserved Words

Modern mode reserves additional keywords:

```
agent const func match proc setvar var
```

These can still be used as command names if quoted or escaped.

## A.4 Compatibility Mode

When run as `colony-sh` or with `--posix`:

1. All POSIX-incompatible syntax is disabled
2. POSIX-compliant defaults are used
3. Colony extensions are unavailable
4. Behavior matches `/bin/sh` expectations

### A.4.1 Enabling Extensions in POSIX Mode

You can selectively enable Colony features:

```sh
#!/bin/colony-sh

# Enable specific features
shopt --set parse_arrow    # Enable -> pipe
shopt --set simple_word_eval  # Disable word splitting

# Or enable a preset
shopt --set colony:upgrade  # Safe subset of extensions
```

## A.5 Testing POSIX Compliance

Colony includes a POSIX compliance test suite:

```bash
# Run POSIX compliance tests
cargo test --package colony-parser posix::
cargo test --package colony-exec posix::

# Run against POSIX test suite (external)
./tests/posix/run-compliance.sh
```

### A.5.1 Test Categories

| Category | Tests | Status |
|----------|-------|--------|
| Lexical | 47 | ✅ Passing |
| Expansion | 52 | ✅ Passing |
| Commands | 38 | ✅ Passing |
| Redirections | 24 | ✅ Passing |
| Control Flow | 31 | ✅ Passing |
| Functions | 18 | ✅ Passing |
| Builtins | 45 | 🚧 35 passing |
| Job Control | 22 | 🚧 12 passing |

## A.6 Migration from bash/zsh

### A.6.1 bash Features Supported

| Feature | Support |
|---------|---------|
| Arrays (`array=()`) | ✅ |
| Associative arrays | ✅ |
| `[[` conditional | ✅ |
| `(())` arithmetic | ✅ |
| Brace expansion | ✅ |
| `<()` process sub | ✅ |
| `$'...'` strings | ✅ |
| Extended globs | 🚧 |

### A.6.2 zsh Features Supported

| Feature | Support |
|---------|---------|
| Anonymous functions | ✅ (via lambda) |
| Floating point | ✅ |
| Glob qualifiers | 🚧 |
| Parameter flags | 🚧 |

## A.7 References

- [POSIX.1-2017 Shell Command Language](https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html)
- [Bash Reference Manual](https://www.gnu.org/software/bash/manual/)
- [Oil Shell POSIX](https://www.oilshell.org/blog/2020/10/osh-features.html)

---

*Return to [Overview](0-overview.md)*
