# Colony Shell Tutorial Mode: Script X-Ray

**Date:** 2025-12-24
**Status:** Design Document
**Supersedes:** 2025-12-24-tutorial-mode.md (structured lessons approach)

---

## Overview

Tutorial mode provides **X-ray vision into any shell script's execution**. Rather than curated lessons teaching shell concepts, this is a transparency tool that lets you watch the magic happening inside any script — parsing, expansion, execution — step by step.

**Invocation:**
```bash
colony --tutorial script.sh
colony --tutorial script.sh --teach  # First-occurrence teaching mode
```

## Philosophy

**"Glass Box" Shell:** Traditional shells are black boxes. You type commands, things happen. Tutorial mode makes the invisible visible — you see *how* the shell interprets your script, not just *what* it outputs.

**Primary Use Cases:**
1. **Learning** — "I don't understand shell syntax, show me what this script does"
2. **Curiosity** — "I know shells, but I want to appreciate the complexity/beauty"
3. **Debugging** — Natural side benefit of full transparency

**Design Principles:**
- Works on **any** shell script, not just Colony-specific features
- Flows naturally with smart auto-detection of interesting moments
- Depth on demand — shell mechanics by default, deep internals when you want them
- Showcases Colony's channel architecture (fd 6 stddiag)

---

## User Experience

### Split Pane TUI

The primary interface is a side-by-side view:

```
┌─ script.sh ─────────────────┬─ Magic ──────────────────────────────┐
│                             │                                       │
│  1  #!/bin/bash             │  Shebang: /bin/bash                  │
│  2                          │  (Colony interpreting)                │
│  3▸ name="${1:-default}"    │                                       │
│  4  echo "Hello, $name"     │  ┌─ EXPANSION ─────────────────────┐ │
│  5                          │  │ ${1:-default}                    │ │
│  6  for f in *.txt; do      │  │   $1 = <unset>                   │ │
│  7    wc -l "$f"            │  │   :- means "use if unset"        │ │
│  8  done                    │  │   Result: "default"              │ │
│                             │  └──────────────────────────────────┘ │
│                             │                                       │
│                             │  ASSIGN: name="default"               │
├─────────────────────────────┴───────────────────────────────────────┤
│ [space] pause  [enter] step  [d] depth  [m] mark  [q] quit         │
└─────────────────────────────────────────────────────────────────────┘
```

**Left pane:** Script source with current line highlighted (`▸`)
**Right pane:** Magic annotations explaining what's happening
**Bottom bar:** Keyboard controls

### Playback Flow

The script plays back at a readable pace — not instant, not tediously slow. Simple statements flow by with brief annotations. Complex constructs (pipes, expansions, loops) slow down automatically with richer detail.

You're watching a movie with intelligent subtitles that know when to say more.

---

## Interaction Model

### Keyboard Controls (Primary)

| Key | Action |
|-----|--------|
| `space` | Pause / Resume playback |
| `enter` | Step one statement (when paused) |
| `d` | Toggle depth (Level 2 ↔ Level 3) |
| `m` | Mark current line for deep dive |
| `/` | Search for pattern in script |
| `t` | Toggle teach mode (first-occurrence explanations) |
| `q` | Quit |

When paused, the magic pane shows full detail for the current statement. Press `enter` to step through one statement at a time, or `space` to resume flow.

### Mouse Enhancement (Optional)

For terminals with mouse support:

- **Click** on any token (variable, operator, command) to expand its explanation
- **Hover** over constructs for quick tooltip info
- **Click** on variables to see current value and assignment history

Mouse is an enhancement, not a requirement. Full functionality via keyboard alone.

### Marking Points of Interest

Press `m` while on any line to mark it. Marked lines always get full attention on replay, regardless of complexity scoring. Useful for:

- "I don't care that this looks simple, explain it"
- Building a personal "explain these parts" list
- Re-running with focus on specific sections

Marks can be saved to a sidecar file (`.script.sh.colony-marks`) for persistent interest tracking.

---

## Depth Levels

Toggle between levels with `[d]`. Default is Level 2.

### Level 2: Shell Mechanics (Default)

Shows what the shell is doing in human-readable terms:

```
Line 5: files=$(find . -name "*.rs" | head -5)

  PARSE:    Assignment with command substitution + pipeline
  EXPAND:   $(...) triggers subshell
  EXECUTE:  find . -name "*.rs" | head -5
            └─ find: searching for *.rs files
            └─ head: taking first 5 lines
  CAPTURE:  stdout → "src/main.rs\nsrc/lib.rs\n..."
  ASSIGN:   files="src/main.rs
                   src/lib.rs
                   src/parser.rs
                   src/exec.rs
                   src/expand.rs"
```

Enough to understand *what* and *why*, without drowning in implementation details.

### Level 3: Deep Internals

For the curious — shows AST structure and syscall-level activity:

```
Line 5: files=$(find . -name "*.rs" | head -5)

  AST: Assignment {
         name: "files",
         value: Word([CommandSub(Pipeline {
           commands: [
             SimpleCommand { name: "find", args: [".", "-name", "*.rs"] },
             SimpleCommand { name: "head", args: ["-5"] }
           ]
         })])
       }

  SYSCALL: pipe([3, 4])           # pipe between find and head
  SYSCALL: pipe([5, 6])           # capture pipe for substitution
  SYSCALL: fork() = 1001          # find process
  SYSCALL: fork() = 1002          # head process
  SYSCALL: execve("/usr/bin/find", ...)
  SYSCALL: execve("/usr/bin/head", ...)
  SYSCALL: read(5, "src/main.rs\n...", 4096) = 73
  SYSCALL: wait4(1001) = 0
  SYSCALL: wait4(1002) = 0
```

Level 3 is opt-in. For learning shell syntax, Level 2 suffices. Level 3 is for understanding shell *implementation*.

---

## Smart Auto-Detection

Not all statements deserve equal attention. The tutorial engine scores complexity and adjusts pacing automatically.

### Complexity Signals

| Signal | Score Impact |
|--------|--------------|
| Simple command (`echo`, `cd`, `true`) | Low |
| Variable assignment (`x=1`) | Low |
| Parameter expansion (`${var:-default}`) | Medium |
| Command substitution (`$(...)`) | Medium |
| Pipelines (`a \| b \| c`) | Medium-High |
| Redirections (`2>&1`, `<()`) | Medium |
| Loops (`for`, `while`) | High (on entry) |
| Conditionals (`if`, `case`) | High (on branch) |
| Nested constructs | Multiplier |

### Pacing Behavior

**Low complexity:** Brief annotation, flows past in ~0.5s
```
Line 12: echo "Done"
  → prints "Done" to stdout
```

**Medium complexity:** Fuller annotation, ~1-2s pause
```
Line 8: count=$(wc -l < "$file")
  EXPAND:   $file → "data.txt"
  REDIRECT: stdin from data.txt
  EXECUTE:  wc -l
  CAPTURE:  "42"
  ASSIGN:   count="42"
```

**High complexity:** Detailed breakdown, waits for acknowledgment or longer pause
```
Line 15: for f in "${files[@]}"; do
  LOOP ENTRY: iterating over array 'files'
  Elements: ["a.txt", "b.txt", "c.txt"]
  Iteration 1: f="a.txt"
```

### User Overrides

- **Mark (`m`):** Forces high attention regardless of score
- **Skip pattern:** `/^echo/` — skip all lines matching pattern
- **Teach mode:** First occurrence of any construct gets high attention, subsequent instances abbreviated

---

## First-Occurrence Teaching Mode

Enabled with `--teach` flag or `[t]` toggle during playback.

### Behavior

The first time you encounter a shell construct, it gets full explanation:

```
Line 3: name="${1:-default}"

  ┌─ CONCEPT: Parameter Expansion ──────────────────────────┐
  │                                                          │
  │  ${var:-default} is parameter expansion with default.   │
  │                                                          │
  │  Syntax: ${parameter:-word}                              │
  │  Meaning: If $parameter is unset or null, use "word"    │
  │                                                          │
  │  Here: $1 (first argument) is unset                      │
  │        So we use "default"                               │
  │                                                          │
  │  Other forms:                                            │
  │    ${var:=word}  - assign default if unset              │
  │    ${var:+word}  - use word if var IS set               │
  │    ${var:?word}  - error if unset                       │
  └──────────────────────────────────────────────────────────┘
```

**Second occurrence** in the same script:
```
Line 17: host="${2:-localhost}"

  EXPAND: ${2:-localhost}
          $2=<unset> → "localhost"
```

Brief, assumes you learned it already.

### Concept Tracking

Concepts are tracked per-session:
- Parameter expansion variants (`:-`, `:=`, `:+`, `:?`)
- Pipes and pipelines
- Redirections (`>`, `>>`, `<`, `2>&1`, etc.)
- Command substitution (`$()` vs backticks)
- Arithmetic (`$(( ))`)
- Globs (`*`, `?`, `[...]`)
- Quoting rules (single vs double vs `$'...'`)

Reset with `--teach-reset` to re-learn everything.

---

## External Command Awareness

Tutorial mode focuses on shell mechanics, but external commands aren't black boxes either.

### What We Show

For any external command, we display:
- Full resolved path (`/usr/bin/find`)
- Arguments after expansion
- What we're piping in/out

```
Line 8: find . -name "*.rs" -type f

  RESOLVE: find → /usr/bin/find
  ARGS:    [".", "-name", "*.rs", "-type", "f"]
  EXECUTE: spawning child process
  EXIT:    0 (success)
```

### Smart Explanations for Common Commands

For frequently-used commands, offer brief contextual hints:

```
Line 12: grep -r "TODO" src/

  RESOLVE: grep → /usr/bin/grep
  ARGS:    ["-r", "TODO", "src/"]
  
  💡 grep -r: recursive search for pattern in files
     Searching for "TODO" in all files under src/
```

Commands with built-in awareness:
- `find`, `grep`, `sed`, `awk` — the text processing gang
- `xargs`, `tee`, `cut`, `sort`, `uniq` — pipeline staples
- `curl`, `wget` — network operations
- `chmod`, `chown`, `rm`, `mv`, `cp` — file operations

### Unknown Commands

For unfamiliar commands, just show the mechanics:
```
Line 20: frobnicate --turbo data.bin

  RESOLVE: frobnicate → /usr/local/bin/frobnicate
  ARGS:    ["--turbo", "data.bin"]
  EXECUTE: spawning child process
  EXIT:    0
```

No guessing. The magic is in showing shell behavior, not inventing explanations.

---

## Architecture

Tutorial mode showcases Colony's extended channel system.

### Data Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                        colony --tutorial                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────────────────────┐  │
│  │  Parser  │───▶│ Executor │───▶│ TraceEmitter             │  │
│  └──────────┘    └──────────┘    │                          │  │
│                                   │ Emits structured events  │  │
│                                   │ to fd 6 (stddiag)       │  │
│                                   └────────────┬─────────────┘  │
│                                                │                 │
│                                                ▼                 │
│                                   ┌──────────────────────────┐  │
│                                   │ fd 6: stddiag           │  │
│                                   │ (structured JSON stream) │  │
│                                   └────────────┬─────────────┘  │
│                                                │                 │
└────────────────────────────────────────────────┼─────────────────┘
                                                 │
                    ┌────────────────────────────┼────────────────────────┐
                    │                            │                        │
                    ▼                            ▼                        ▼
          ┌─────────────────┐         ┌─────────────────┐      ┌─────────────────┐
          │ TUI Renderer    │         │ JSON File       │      │ Web Viewer      │
          │ (split pane)    │         │ (--trace-out)   │      │ (future)        │
          └─────────────────┘         └─────────────────┘      └─────────────────┘
```

### Trace Events (fd 6)

Structured JSON events emitted during execution:

```json
{"type": "line_enter", "line": 3, "source": "name=\"${1:-default}\""}

{"type": "parse", "line": 3, "ast": "Assignment", "details": {...}}

{"type": "expand", "line": 3, "phase": "parameter", 
 "input": "${1:-default}", "output": "default", 
 "explanation": "$1 unset, using default"}

{"type": "assign", "line": 3, "name": "name", "value": "default"}

{"type": "line_exit", "line": 3, "duration_us": 142}
```

### Why fd 6 (stddiag)?

Colony's channel system:
- fd 3: stdcontext (context accumulation)
- fd 4: stdllm (LLM communication)
- fd 5: stdtool (tool calls)
- fd 6: stddiag (execution tracing) ← **Tutorial mode lives here**

This means:
- Script's stdout/stderr flow normally
- Trace data is separate, structured, consumable
- Multiple consumers can attach (TUI, file, web)
- Real demonstration of Colony's architecture

---

## Module Structure

```
crates/colony-shell/src/tutorial/
├── mod.rs              # Public API, TutorialSession
├── tracer.rs           # TraceEmitter, writes to fd 6
├── events.rs           # Trace event types (JSON serializable)
├── complexity.rs       # Complexity scoring algorithm
├── explain/
│   ├── mod.rs          # Explanation generation
│   ├── constructs.rs   # Shell construct explanations
│   └── commands.rs     # External command hints
├── teach.rs            # First-occurrence tracking
└── tui/
    ├── mod.rs          # TUI orchestration
    ├── layout.rs       # Split pane layout
    ├── script_pane.rs  # Left pane (source display)
    ├── magic_pane.rs   # Right pane (annotations)
    └── controls.rs     # Keyboard/mouse handling

crates/colony-exec/src/
├── executor.rs         # Modified to emit trace events
└── trace_hooks.rs      # Trace injection points
```

### Key Types

```rust
/// Emits structured trace events to fd 6
pub struct TraceEmitter {
    output: RawFd,  // fd 6
    depth: TraceDepth,
    teach_mode: bool,
    seen_concepts: HashSet<Concept>,
}

/// Trace depth levels
pub enum TraceDepth {
    Mechanics,  // Level 2 (default)
    Internals,  // Level 3
}

/// A single trace event
#[derive(Serialize)]
#[serde(tag = "type")]
pub enum TraceEvent {
    LineEnter { line: u32, source: String },
    Parse { line: u32, ast_summary: String, ast_full: Option<Value> },
    Expand { line: u32, phase: String, input: String, output: String, explanation: String },
    Execute { line: u32, command: String, resolved: PathBuf, args: Vec<String> },
    Assign { line: u32, name: String, value: String },
    LineExit { line: u32, duration_us: u64 },
    Concept { name: String, explanation: String },  // teach mode
    // ... more events
}

/// Complexity scorer
pub struct ComplexityScorer;

impl ComplexityScorer {
    pub fn score(stmt: &Statement) -> ComplexityScore {
        // Analyze AST, return Low/Medium/High
    }
}
```

---

## Integration with Executor

Tutorial mode hooks into the executor at key points, without polluting the normal execution path.

### Trace Hooks

```rust
// crates/colony-exec/src/executor.rs

impl Executor {
    pub async fn execute_statement(
        &mut self, 
        stmt: &Statement,
        tracer: Option<&mut TraceEmitter>,  // None in normal mode
    ) -> Result<ExitStatus> {
        
        // Hook: entering statement
        if let Some(t) = tracer {
            t.emit(TraceEvent::LineEnter { 
                line: stmt.span.start.line,
                source: stmt.source_text(),
            });
            t.emit_parse(stmt);
        }
        
        // Normal execution with hooks at expansion/execution points
        let result = self.execute_inner(stmt, tracer).await;
        
        // Hook: exiting statement
        if let Some(t) = tracer {
            t.emit(TraceEvent::LineExit {
                line: stmt.span.start.line,
                duration_us: elapsed.as_micros() as u64,
            });
        }
        
        result
    }
}
```

### Expansion Hooks

```rust
// crates/colony-expand/src/expander.rs

impl Expander {
    pub fn expand_word(
        &mut self,
        word: &Word,
        tracer: Option<&mut TraceEmitter>,
    ) -> Result<Vec<String>> {
        
        for phase in [Tilde, Parameter, Command, Arithmetic, FieldSplit, Pathname] {
            let before = current.clone();
            current = self.apply_phase(phase, current)?;
            
            // Hook: expansion phase
            if let Some(t) = tracer {
                if before != current {
                    t.emit(TraceEvent::Expand {
                        line: self.current_line,
                        phase: phase.name(),
                        input: before.display(),
                        output: current.display(),
                        explanation: phase.explain(&before, &current),
                    });
                }
            }
        }
        
        Ok(current.into_fields())
    }
}
```

### Zero Overhead When Disabled

Tutorial hooks are behind `Option<&mut TraceEmitter>`. When `None`:
- No allocations
- No formatting
- Just a branch prediction that's always false

Normal shell execution pays no cost.

---

## TUI Implementation

The TUI consumes trace events from fd 6 and renders the split-pane interface.

### Layout Engine

```rust
// crates/colony-shell/src/tutorial/tui/layout.rs

pub struct TutorialTui {
    script_pane: ScriptPane,
    magic_pane: MagicPane,
    controls_bar: ControlsBar,
    state: PlaybackState,
}

pub enum PlaybackState {
    Playing { speed: f32 },
    Paused,
    Stepping,
}

impl TutorialTui {
    pub fn new(script_path: &Path) -> Result<Self> {
        let source = fs::read_to_string(script_path)?;
        Ok(Self {
            script_pane: ScriptPane::new(source),
            magic_pane: MagicPane::new(),
            controls_bar: ControlsBar::default(),
            state: PlaybackState::Paused,  // Start paused
        })
    }
    
    pub async fn run(&mut self, trace_rx: Receiver<TraceEvent>) -> Result<()> {
        loop {
            // Handle input
            if let Some(key) = self.poll_input()? {
                match key {
                    Key::Char(' ') => self.toggle_pause(),
                    Key::Enter => self.step(),
                    Key::Char('d') => self.toggle_depth(),
                    Key::Char('m') => self.mark_current_line(),
                    Key::Char('q') => break,
                    _ => {}
                }
            }
            
            // Consume trace events based on playback state
            if let PlaybackState::Playing { speed } = self.state {
                if let Ok(event) = trace_rx.try_recv() {
                    self.handle_event(event, speed).await?;
                }
            }
            
            // Render
            self.render()?;
        }
        Ok(())
    }
}
```

### Script Pane

```rust
// crates/colony-shell/src/tutorial/tui/script_pane.rs

pub struct ScriptPane {
    lines: Vec<String>,
    current_line: u32,
    marked_lines: HashSet<u32>,
    scroll_offset: u32,
}

impl ScriptPane {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let visible_lines = area.height as u32;
        
        for (i, line) in self.visible_lines().enumerate() {
            let line_num = self.scroll_offset + i as u32 + 1;
            
            let style = if line_num == self.current_line {
                Style::default().bg(Color::DarkGray).bold()
            } else if self.marked_lines.contains(&line_num) {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            
            let marker = if line_num == self.current_line { "▸" } else { " " };
            
            frame.render_widget(
                Paragraph::new(format!("{} {:3} {}", marker, line_num, line))
                    .style(style),
                line_area,
            );
        }
    }
}
```

### Magic Pane

```rust
// crates/colony-shell/src/tutorial/tui/magic_pane.rs

pub struct MagicPane {
    sections: Vec<MagicSection>,
    depth: TraceDepth,
}

pub struct MagicSection {
    title: String,
    content: Vec<StyledLine>,
    expanded: bool,
}

impl MagicPane {
    pub fn handle_event(&mut self, event: TraceEvent) {
        match event {
            TraceEvent::Parse { ast_summary, ast_full, .. } => {
                self.sections.push(MagicSection {
                    title: "PARSE".into(),
                    content: self.format_parse(ast_summary, ast_full),
                    expanded: true,
                });
            }
            TraceEvent::Expand { phase, input, output, explanation, .. } => {
                self.sections.push(MagicSection {
                    title: format!("EXPAND ({})", phase),
                    content: self.format_expansion(input, output, explanation),
                    expanded: true,
                });
            }
            TraceEvent::Concept { name, explanation } => {
                // Teach mode: highlighted concept box
                self.sections.push(MagicSection {
                    title: format!("CONCEPT: {}", name),
                    content: self.format_concept(explanation),
                    expanded: true,
                });
            }
            // ... handle other events
        }
    }
    
    pub fn clear_for_new_line(&mut self) {
        self.sections.clear();
    }
}
```

---

## CLI Interface

```bash
# Basic usage
colony --tutorial script.sh

# With teach mode (first-occurrence explanations)
colony --tutorial script.sh --teach

# Start at specific line
colony --tutorial script.sh --start-line 25

# Output trace to file (for later replay or web viewer)
colony --tutorial script.sh --trace-out trace.json

# Replay from trace file (no re-execution)
colony --tutorial-replay trace.json

# Set initial depth
colony --tutorial script.sh --depth internals

# Load marks from previous session
colony --tutorial script.sh --marks script.sh.colony-marks
```

### Configuration

`~/.config/colony/tutorial.toml`:
```toml
[defaults]
depth = "mechanics"  # or "internals"
teach_mode = false
playback_speed = 1.0

[pacing]
low_complexity_ms = 500
medium_complexity_ms = 1500
high_complexity_ms = 3000  # or "pause"

[commands]
# Custom explanations for domain-specific tools
[commands.kubectl]
hint = "Kubernetes CLI"
```

---

## Implementation Roadmap

### Phase 1: Trace Infrastructure
- [ ] Define `TraceEvent` enum with all event types
- [ ] Implement `TraceEmitter` writing to fd 6
- [ ] Add trace hooks to executor (behind Option)
- [ ] Add trace hooks to expander
- [ ] Basic JSON serialization

### Phase 2: Complexity Scoring
- [ ] Implement `ComplexityScorer` for AST analysis
- [ ] Score all statement types
- [ ] Handle nested complexity multipliers
- [ ] Unit tests for scoring

### Phase 3: TUI Foundation
- [ ] Split pane layout with ratatui
- [ ] Script pane with line numbers and highlighting
- [ ] Magic pane with section rendering
- [ ] Keyboard controls (space, enter, q)
- [ ] Basic playback state machine

### Phase 4: Playback & Pacing
- [ ] Consume trace events with pacing
- [ ] Complexity-based auto-pacing
- [ ] Pause/step/resume flow
- [ ] Line marking

### Phase 5: Explanations
- [ ] Shell construct explanations (parameter expansion, etc.)
- [ ] External command hints (find, grep, etc.)
- [ ] Teach mode with concept tracking
- [ ] Level 2 vs Level 3 formatting

### Phase 6: Polish
- [ ] Mouse support (click to expand)
- [ ] Search (`/`)
- [ ] Marks persistence
- [ ] Trace file export/replay
- [ ] Configuration file support

### Phase 7: Future
- [ ] Web viewer for trace files
- [ ] Shareable tutorial links
- [ ] AI-powered explanations for unknown commands
- [ ] Integration with Colony terminal for richer rendering

---

## Relationship to Existing Tutorial Code

The previous design (`2025-12-24-tutorial-mode.md`) focused on **curated lessons** for teaching shell concepts. This "Script X-Ray" approach is different:

| Structured Lessons | Script X-Ray |
|--------------------|--------------|
| Pre-defined curriculum | Any script |
| "Let me teach you shells" | "Let me show you what this does" |
| Validates user input | Observes execution |
| Progressive difficulty | Complexity auto-detected |

The approaches are complementary. Structured lessons could use Script X-Ray internally for their "magic reveal" moments. For now, we focus on Script X-Ray as the core capability.

---

**Document Status:** Ready for Implementation
**Dependencies:** ratatui (TUI), serde_json (events), crossterm (input)
