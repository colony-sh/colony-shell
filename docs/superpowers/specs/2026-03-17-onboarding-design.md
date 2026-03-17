# Colony Shell Onboarding — Design Spec

**Date:** 2026-03-17
**Status:** Approved

## Overview

Two-part onboarding: a first-run wizard that fires on initial launch, and a `colony onboard` tutorial that can be revisited anytime.

---

## Part 1: First-Run Wizard

### Detection

No `~/.colony/` directory exists. Shell detects this in the REPL before showing the first prompt.

### Flow

```
Welcome to Colony Shell v0.4.1
An AI-native shell for developers and agents.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

[1/5] Import existing shell config?

  We found:
    ~/.zshrc (247 lines)
    ~/.zprofile (12 lines)

  Colony can import your aliases, environment variables,
  functions, and PATH config so everything works immediately.

  > Import from zsh [recommended]
    Start fresh
    Skip (decide later)

[2/5] Choose your keybinding style

  > Vim (default)
    Emacs
    Hybrid (vim normal + emacs insert)

[3/5] LLM Provider

  Colony's agent needs an API key. Which provider?

  > Anthropic (Claude)
    OpenAI (GPT-4)
    Ollama (local models)
    Skip (configure later)

  Enter API key: sk-ant-***
  ✓ Verified — claude-sonnet-4-5-20250514 responding

[4/5] Claude Code Integration

  Colony includes a Claude Code plugin that gives Claude
  awareness of your shell environment and canvas.

  > Install plugin to ~/.claude/plugins/colony/
    Skip

  ✓ Plugin installed — run /reload-plugins in Claude Code

[5/5] Quick Tour?

  Want a 2-minute interactive tour of Colony's features?

  > Yes, show me around
    No, drop me at the prompt

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✓ Colony Shell is ready. Type # or ? to talk to the agent.
```

### What Gets Created

```
~/.colony/
├── config.toml          # User preferences (keybindings, theme, provider)
├── profile              # Imported shell config (aliases, exports, functions, PATH)
├── context/             # Persistent context store
├── recipes/             # Recipe registry
└── history.db           # Command history (SQLite)
```

### config.toml

```toml
[shell]
keybindings = "vim"       # vim | emacs | hybrid
theme = "colony-dark"

[agent]
provider = "anthropic"
model = "claude-sonnet-4-5-20250514"
# API key stored in system keychain, NOT in config file

[onboarding]
completed = true
completed_at = "2026-03-17T10:30:00Z"
imported_from = "zsh"
```

---

## Part 2: Profile Converter

### Detection

Scans for these files in order:
1. `~/.zshrc` + `~/.zprofile` + `~/.zshenv`
2. `~/.bashrc` + `~/.bash_profile` + `~/.profile`

### What It Imports

| Category | Source | Action |
|----------|--------|--------|
| **Aliases** | `alias ll='ls -la'` | Direct copy — Colony aliases are POSIX-compatible |
| **Exports** | `export EDITOR=vim` | Direct copy |
| **PATH mutations** | `export PATH="$HOME/.cargo/bin:$PATH"` | Merge into single PATH statement, dedup |
| **Functions** | `function mkcd() { ... }` | Copy — Colony functions are bash-compatible |
| **Tool inits** | `eval "$(brew shellenv)"` | Detect and translate to Colony equivalent |
| | `eval "$(starship init zsh)"` | Detect Starship, configure Colony prompt integration |
| | `eval "$(pyenv init -)"` | Copy — these work in Colony |
| | `eval "$(nvm init)"` / nvm.sh source | Copy with note |
| | `eval "$(rbenv init -)"` | Copy |
| | `eval "$(direnv hook zsh)"` | Copy — Colony has .colonyrc but direnv still works |
| **Completions** | zsh completions | Skip — Colony has its own completion system |
| **Prompt (PS1)** | `PS1=...` | Convert to Colony prompt format or note Starship |
| **Key bindings** | `bindkey` (zsh) / `.inputrc` (bash) | Map to Colony keybinding config |
| **Oh My Zsh** | `plugins=(git docker node)` | Detect, note in comments — Colony has its own plugins |
| **Zsh-specific** | `setopt`, `autoload`, `compinit` | Skip with comment explaining Colony equivalent |

### Output Format

`~/.colony/profile` is sourced on shell startup:

```bash
# Colony Profile — imported from ~/.zshrc on 2026-03-17
# Lines marked [IMPORTED] were converted automatically.
# Lines marked [SKIPPED] need manual review.

# ── Aliases ──────────────────────────────────
alias ll='ls -la'                    # [IMPORTED]
alias gs='git status'                # [IMPORTED]
alias gp='git push'                  # [IMPORTED]

# ── Environment ──────────────────────────────
export EDITOR="nvim"                 # [IMPORTED]
export VISUAL="nvim"                 # [IMPORTED]
export LANG="en_US.UTF-8"           # [IMPORTED]

# ── PATH ─────────────────────────────────────
# Merged from multiple export statements, duplicates removed
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.pyenv/bin:/opt/homebrew/bin:$PATH"

# ── Functions ────────────────────────────────
mkcd() {                             # [IMPORTED]
    mkdir -p "$1" && cd "$1"
}

# ── Tool Initialization ─────────────────────
eval "$(starship init bash)"         # [IMPORTED] Starship detected — works with Colony
eval "$(pyenv init -)"               # [IMPORTED]

# ── Skipped ──────────────────────────────────
# setopt AUTO_CD                     # [SKIPPED] Colony: use `set -o autocd` instead
# bindkey '^R' history-search        # [SKIPPED] Colony: Ctrl+R built-in (history search)
# compinit                           # [SKIPPED] Colony has its own completion engine
# plugins=(git docker node)          # [SKIPPED] Oh My Zsh plugins — Colony uses recipes
```

### Conflict Handling

If `~/.colony/profile` already exists:
- Show diff of what would change
- Ask: Overwrite / Merge / Skip

---

## Part 3: Interactive Tutorial (`colony onboard`)

Uses the X-Ray debugger in a guided mode. Runs a small demonstration script with annotations explaining each Colony feature.

### Tutorial Script

```bash
#!/bin/bash
# Colony Shell Feature Tour

# 1. Basic shell — it's a normal shell
echo "Colony is a POSIX-compatible shell"
ls -la

# 2. Context system
context add "This project uses Rust and Swift"
context show

# 3. Agent interaction
# The agent uses your configured LLM provider
echo "? what files are in this directory"

# 4. Channels
echo "Channels accumulate session data"
llm show --last 3
tool show
diag show

# 5. Builtins
agent status
canvas status 2>/dev/null || echo "(canvas available when connected to Colony Nerve)"

# 6. Keyboard shortcuts
echo "Key shortcuts:"
echo "  Ctrl+R  — history search"
echo "  Ctrl+P  — file picker"
echo "  Cmd+Shift+P — command palette"
echo "  Tab     — accept completion"
```

The X-Ray system runs this with `--teach` mode enabled, pausing at each section to explain what's happening. The user presses Enter to advance.

### Invocation

```bash
colony onboard        # Run the interactive tutorial
colony onboard --skip-wizard  # Just the tutorial, no setup
colony --tutorial onboard.sh  # Same thing via X-Ray
```

---

## Implementation

### New Files

```
crates/colony-shell/src/onboarding/
├── mod.rs              # Orchestrator: detect first run, run wizard
├── wizard.rs           # Step-by-step wizard (TUI or simple terminal prompts)
├── converter.rs        # Profile converter (read bash/zsh, write colony profile)
├── importer.rs         # Parse bash/zsh config files (aliases, exports, functions, PATH)
└── tutorial.rs         # Tutorial script runner (wraps X-Ray)
```

### Existing Files to Modify

- `crates/colony-shell/src/repl.rs` — check for first run before entering REPL
- `crates/colony-exec/src/builtins.rs` — add `colony onboard` subcommand
- `crates/colony-shell/src/cli.rs` — add `--onboard` flag
