# Panel-as-Process Architecture

## Overview

Colony's TUI becomes a composable layout of panels. The main panel is the REPL and owns the terminal directly. All other panels are long-lived child processes that render via a structured JSON protocol on stdout and receive input events on stdin. Panels communicate with each other through the tuple space.

## The `sidebar` Command

```
sidebar <position> [options] <command    # add a panel
sidebar reset                            # clear all panels
sidebar reset <position>                 # clear panels at position
```

### Positions

`left`, `right`, `top`, `bottom`

### Options

| Option | Applies to | Description |
|--------|-----------|-------------|
| `--outer` | top/bottom | Full width instead of inner (between left/right panels) |
| `--weight N` | all | Relative size when sharing space (default 1) |
| `--min-width N` | left/right | Minimum column width |
| `--max-width N` | left/right | Maximum column width |
| `--min-height N` | top/bottom | Minimum row height |
| `--max-height N` | top/bottom | Maximum row height |

## Layout Model

```
+--------------------------------------------------+
|              outer top panels                    |
+--------+----------------------------+-----------+
|        |      inner top panels     |           |
|  left  +----------------------------+  right    |
| panels |      main (REPL)          |  panels   |
|        +----------------------------+           |
|        |    inner bottom panels    |           |
+--------+----------------------------+-----------+
|             outer bottom panels                 |
+--------------------------------------------------+
```

### Layout resolution order

1. Outer top panels (full width, stacked vertically)
2. Outer bottom panels (full width, stacked vertically)
3. Remaining vertical space split: left panels | main column | right panels
4. Within main column: inner top, main REPL, inner bottom

### Sizing

- Multiple panels at the same position stack **horizontally** (left/right) or **vertically** (top/bottom)
- Width/height is content-driven, constrained by min/max, distributed by weight
- Default constraints: left/right min-width 15, max-width 40; top/bottom min-height 3, max-height 15
- Main panel enforced minimum: 10 columns wide, 5 rows tall

## Panel Protocol

### stdout: process to host

JSON lines delimited by `---FRAME---`. The host replaces panel content atomically on each complete frame.

```
---FRAME---
{"text":"..","icon":"\u{f0162}","style":"dim","action":"cd_parent"}
{"text":"src","icon":"\u{f024b}","style":"bold,blue","expandable":true,"expanded":true,"action":"toggle:/path/src"}
{"text":"main.rs","icon":"\u{e7a8}","style":"yellow","indent":1,"action":"open:/path/main.rs"}
---FRAME---
```

#### Line fields

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `text` | yes | string | Display text |
| `icon` | no | string | Icon prefix |
| `style` | no | string | Comma-separated: `bold`, `dim`, `italic`, color names (`red`, `blue`), hex (`#rrggbb`) |
| `indent` | no | number | Indentation level (default 0) |
| `action` | no | string | Action string for Enter/click |
| `expandable` | no | bool | Shows expand/collapse indicator |
| `expanded` | no | bool | Current expand state |

### stdin: host to process

JSON events, one per line.

```json
{"event":"key","code":"j"}
{"event":"key","code":"Enter","modifiers":"ctrl"}
{"event":"resize","width":25,"height":40}
{"event":"focus","focused":true}
{"event":"shutdown"}
```

#### Event types

| Event | Fields | Description |
|-------|--------|-------------|
| `key` | `code`, `modifiers` | Keystroke when panel is focused |
| `resize` | `width`, `height` | Panel dimensions changed |
| `focus` | `focused` | Panel gained or lost focus |
| `shutdown` | none | Host is shutting down, process should exit |

## Panel Lifecycle

1. `sidebar` spawns the command as a child process
2. Host reads stdout for frames, renders panel content on each complete frame
3. Host writes keystrokes to stdin when panel is focused
4. Process subscribes/publishes to tuple space independently
5. On process exit or error, panel is removed from layout

## Focus Model

- Tab cycles through panels
- Smart tab: empty prompt switches focus, typing falls through to completion
- Focused panel receives keystrokes via stdin
- Unfocused panels still receive tuple space events and can re-render

## Main Panel

The main panel is special-cased. It owns the terminal directly and runs the REPL. It does not use the panel protocol. All other panels use the structured protocol.

## Shell Init: `.colony/init`

A shell script that runs at startup. Directory-scoped, layered:

1. `~/.colony/init` runs first (global defaults)
2. `.colony/init` in project directory runs second (layers on top)

Contains any shell commands: sidebar layout, aliases, PATH modifications, exports.

```sh
# ~/.colony/init
export PATH="$HOME/.local/bin:$PATH"
alias ll="ls -la"
sidebar left <filetree

# ~/myproject/.colony/init (layers on global)
sidebar left <filedetails --max-width 30
sidebar right <preview
sidebar bottom <gitstage
```

Use `sidebar reset` at the top of a project init to replace the global layout entirely:

```sh
# ~/myproject/.colony/init (replaces global layout)
sidebar reset
sidebar left <gitstatus
sidebar right <preview
sidebar bottom --outer <logs
```

## Panel Binaries

Colony builtins that follow the panel protocol. Each is a standalone command.

| Binary | Typical position | Description | Tuple space |
|--------|-----------------|-------------|-------------|
| `filetree` | left | File browser with expand/collapse, git status, `..` navigation | Publishes `("filetree_select", path)` on cursor move |
| `filedetails` | left | File size, permissions, git blame summary | Subscribes `("filetree_select", path)` |
| `preview` | right | Read-only syntax-highlighted file preview | Subscribes `("filetree_select", path)` |
| `gitstatus` | left/right | Modified, staged, untracked files list | Subscribes `("cmd_complete")`, `("git_change")` |
| `gitstage` | bottom | Inline diff of selected file, stage/unstage hunks | Subscribes `("gitfiles_select", path)` |
| `jobs` | bottom | Running and stopped background jobs | Subscribes `("job_change")` |
| `history` | right | Searchable command history | Subscribes `("cmd_complete")` |
| `env` | right | Environment variables, filterable | Subscribes `("env_change")` |
| `logs` | bottom --outer | Scrolling log output from long-running commands | Subscribes `("cmd_output")` |
| `session` | right | Token usage, cost estimate, command count | Subscribes `("agent_tokens")`, `("cmd_complete")` |

## Example Configurations

### Basic (current behavior)

```sh
sidebar left <filetree
```

### IDE-like

```sh
sidebar left <filetree
sidebar left <filedetails --max-width 30
sidebar right <preview
sidebar bottom <gitstage
```

### Ops/monitoring

```sh
sidebar right <jobs
sidebar right <env
sidebar bottom --outer <logs
```

## Integration with Existing Systems

### Tuple Space

stdout is for rendering. Tuple space is for inter-panel communication. These are separate concerns. Panel processes access the tuple space through the existing colony tuple space API.

### `.colonyrc`

Unchanged. `.colonyrc` remains TOML-based configuration for agent personality, MCP servers, tool classifications, and completion settings. `.colony/init` handles shell commands.

### Current Sidebar Code

The existing `SidebarManager`, `SidebarComponent` trait, `FileTreePanel`, and `SidebarRenderer` will be replaced by the new panel host and protocol. The rendering concepts (ComponentLine, styled spans, interactive config) inform the protocol design but the trait-based component system is superseded.
