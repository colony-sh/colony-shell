# Colony Shell Tutorial Mode

**Date:** 2025-12-24  
**Status:** Design & Implementation Plan  
**Priority:** User Onboarding, Education

---

## Executive Summary

Tutorial mode is an interactive learning system that teaches users both POSIX shell fundamentals AND Colony's unique AI-native features through progressive, hands-on lessons. Unlike traditional documentation, users LEARN BY DOING in a safe, guided environment with immediate visual feedback.

**Core Philosophy:** Make the invisible visible. Show users the "magic" happening under the hood — parsing, expansion, tuple emissions, agent coordination — to build deep understanding.

---

## Design Goals

### 1. Progressive Disclosure
- Start with absolute basics (commands, files)
- Gradually reveal complexity (pipelines → control flow → agents)
- Never overwhelm; earn each new concept

### 2. Learn-by-Doing
- No passive reading — every lesson requires interaction
- Immediate feedback on correctness
- Show internal state changes visually

### 3. Multiple Learning Paths
```
┌─────────────────────────────────────────────────────────┐
│  "I know bash, show me Colony features"                 │
│    → Skip to lessons 10+ (Typed Pipes, Agents, Tuples)  │
│                                                          │
│  "I'm new to shells, start from basics"                 │
│    → Begin with lesson 1 (What is a shell?)             │
│                                                          │
│  "I want to understand the internals"                   │
│    → Deep-dive mode with AST visualization              │
└─────────────────────────────────────────────────────────┘
```

### 4. Visual Excellence
- Terminal-native but beautiful
- Box-drawing characters for structure
- ANSI colors for semantic highlighting
- Smooth transitions between states
- Clear visual hierarchy

---

## User Journey Maps

### Path 1: Shell Beginner

```
Lesson 1:  What is a shell?
           ├─ Concept: Command interpreter
           ├─ Exercise: Type your first command
           └─ Magic revealed: How the shell found 'ls'

Lesson 2:  Files and Directories
           ├─ Concept: Filesystem as tree
           ├─ Exercise: Navigate with cd, list with ls
           └─ Magic revealed: Current working directory

Lesson 3:  Arguments and Options
           ├─ Concept: Commands take inputs
           ├─ Exercise: ls -la, echo "hello world"
           └─ Magic revealed: Argument parsing

Lesson 4:  Expansion (Variables)
           ├─ Concept: $name becomes value
           ├─ Exercise: name="Colony"; echo "$name"
           └─ Magic revealed: Expansion order

Lesson 5:  Pipelines
           ├─ Concept: Chain commands together
           ├─ Exercise: ls | grep .rs | wc -l
           └─ Magic revealed: Stdout → stdin flow

Lesson 6:  Redirections
           ├─ Concept: Send output to files
           ├─ Exercise: echo "test" > file.txt
           └─ Magic revealed: File descriptor manipulation

Lesson 7:  Control Flow (if/while/for)
           ├─ Concept: Make decisions, repeat actions
           ├─ Exercise: for i in 1 2 3; do echo $i; done
           └─ Magic revealed: Loop execution

Lesson 8:  Functions
           ├─ Concept: Reusable code blocks
           ├─ Exercise: Define and call a function
           └─ Magic revealed: Function scope

Lesson 9:  Scripts
           ├─ Concept: Save commands to file
           ├─ Exercise: Write and execute a script
           └─ Magic revealed: Shebang and PATH
```

### Path 2: Bash Expert → Colony Features

```
Lesson 10: Extended Channels (fd 3-6)
           ├─ Concept: Semantic channels beyond stdout/stderr
           ├─ Exercise: echo "context" >&3 && context show
           └─ Magic revealed: Channel manager, tuple sync

Lesson 11: Reactive Environment
           ├─ Concept: State changes emit tuples
           ├─ Exercise: cd /tmp (observe tuple emission)
           └─ Magic revealed: Tuple space pub/sub

Lesson 12: Natural Language Detection
           ├─ Concept: # or ? prefix triggers agent
           ├─ Exercise: # find all rust files
           └─ Magic revealed: Parser fallback → LLM

Lesson 13: Agent Integration
           ├─ Concept: Agents as first-class citizens
           ├─ Exercise: agent status, llm use provider:model
           └─ Magic revealed: Multi-provider routing

Lesson 14: Typed Pipelines
           ├─ Concept: -> vs => vs ~>
           ├─ Exercise: find . -name "*.rs" -> count
           └─ Magic revealed: Type annotations flow

Lesson 15: MCP Tool Integration
           ├─ Concept: Call external tools with types
           ├─ Exercise: Use MCP server via agent
           └─ Magic revealed: TypeScript codegen

Lesson 16: Project Configuration (.colonyrc)
           ├─ Concept: Per-project agent personality
           ├─ Exercise: Create .colonyrc with goals
           └─ Magic revealed: Trust model (direnv-style)
```

### Path 3: Internals Deep-Dive

```
Lesson 17: Lexer and Parser
           ├─ Concept: Text → Tokens → AST
           ├─ Exercise: Visualize AST for complex command
           └─ Magic revealed: Pure PEG parser (pest)

Lesson 18: Expansion Order (Deep)
           ├─ Concept: tilde → parameter → command → arithmetic → field split → pathname → quote removal
           ├─ Exercise: Trace expansion step-by-step
           └─ Magic revealed: IntermediateWord tracking

Lesson 19: Process Execution
           ├─ Concept: fork/exec, process groups
           ├─ Exercise: Visualize process tree
           └─ Magic revealed: nix crate syscalls

Lesson 20: Job Control
           ├─ Concept: fg/bg/jobs, signal handling
           ├─ Exercise: Background a job, bring to foreground
           └─ Magic revealed: SIGCHLD, job table
```

---

## Screen Mockups (ASCII Art)

### Lesson Start Screen

```
┌─────────────────────────────────────────────────────────────────┐
│                   Colony Shell Tutorial                         │
│                                                                 │
│  Lesson 4: Variable Expansion                                   │
│  ═══════════════════════════════════════                        │
│                                                                 │
│  GOAL: Understand how Colony expands variables in commands     │
│                                                                 │
│  Variables store values that you can reuse. The shell          │
│  replaces $name with the variable's value before running       │
│  the command.                                                   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ TRY THIS                                                │   │
│  │                                                         │   │
│  │ Set a variable and use it:                             │   │
│  │                                                         │   │
│  │   name="Colony"                                         │   │
│  │   echo "Hello, $name!"                                  │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  Type the commands above, or press [Tab] to auto-fill          │
│                                                                 │
│  colony-tutorial> _                                             │
│                                                                 │
│  ────────────────────────────────────────────────────────────  │
│  [Tab] Auto-fill  [Ctrl+H] Hint  [Ctrl+S] Skip  [Esc] Exit    │
└─────────────────────────────────────────────────────────────────┘
```

### After Correct Input (Step 1)

```
┌─────────────────────────────────────────────────────────────────┐
│                   Colony Shell Tutorial                         │
│                                                                 │
│  Lesson 4: Variable Expansion                                   │
│  ═══════════════════════════════════════════════════════════   │
│                                                                 │
│  colony-tutorial> name="Colony"                                 │
│                                                                 │
│  ✓ Variable assigned: name="Colony"                            │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ WHAT HAPPENED                                            │  │
│  │                                                          │  │
│  │ The shell parsed your input and created an              │  │
│  │ assignment in the environment:                           │  │
│  │                                                          │  │
│  │   name ───▶ "Colony"                                     │  │
│  │                                                          │  │
│  │ This variable is now available for expansion.           │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  Now type the second command:                                   │
│                                                                 │
│  colony-tutorial> _                                             │
│                                                                 │
│  ────────────────────────────────────────────────────────────  │
│  [Tab] Auto-fill  [Ctrl+H] Hint                                │
└─────────────────────────────────────────────────────────────────┘
```

### After Correct Input (Step 2 - Magic Revealed)

```
┌─────────────────────────────────────────────────────────────────┐
│                   Colony Shell Tutorial                         │
│                                                                 │
│  Lesson 4: Variable Expansion                                   │
│  ═══════════════════════════════════════════════════════════   │
│                                                                 │
│  colony-tutorial> echo "Hello, $name!"                          │
│                                                                 │
│  ✓ Correct!                                                     │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ THE MAGIC: How Expansion Works                           │  │
│  │                                                          │  │
│  │ Step 1: PARSE                                            │  │
│  │   echo "Hello, $name!"                                   │  │
│  │   ───▶ SimpleCommand {                                   │  │
│  │          name: "echo",                                   │  │
│  │          args: [Word([Literal("Hello, "),                │  │
│  │                      Dollar("name"),                     │  │
│  │                      Literal("!")])]                     │  │
│  │        }                                                 │  │
│  │                                                          │  │
│  │ Step 2: EXPAND                                           │  │
│  │   "Hello, $name!"                                        │  │
│  │           ╰─────╯                                        │  │
│  │              │                                           │  │
│  │              ▼                                           │  │
│  │   "Hello, Colony!"    (lookup in environment)           │  │
│  │                                                          │  │
│  │ Step 3: EXECUTE                                          │  │
│  │   /bin/echo "Hello, Colony!"                            │  │
│  │                                                          │  │
│  │ Output: Hello, Colony!                                   │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  Hello, Colony!                                                 │
│                                                                 │
│  ✓ Lesson 4 Complete!                                           │
│                                                                 │
│  [Enter] Next Lesson  [R] Repeat  [M] Menu                     │
└─────────────────────────────────────────────────────────────────┘
```

### Hint System

```
┌─────────────────────────────────────────────────────────────────┐
│                   Colony Shell Tutorial                         │
│                                                                 │
│  Lesson 4: Variable Expansion                                   │
│  ═══════════════════════════════════════════════════════════   │
│                                                                 │
│  colony-tutorial> name=                                         │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ 💡 HINT                                                   │  │
│  │                                                          │  │
│  │ To assign a value to a variable, use:                   │  │
│  │                                                          │  │
│  │   variable_name=value                                    │  │
│  │                                                          │  │
│  │ No spaces around the =                                   │  │
│  │                                                          │  │
│  │ If the value has spaces, use quotes:                    │  │
│  │   name="Colony Shell"                                    │  │
│  │                                                          │  │
│  │ Try typing: name="Colony"                                │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  colony-tutorial> name=_                                        │
│                                                                 │
│  ────────────────────────────────────────────────────────────  │
│  [Ctrl+H] More hints                                            │
└─────────────────────────────────────────────────────────────────┘
```

### Error Handling

```
┌─────────────────────────────────────────────────────────────────┐
│                   Colony Shell Tutorial                         │
│                                                                 │
│  Lesson 4: Variable Expansion                                   │
│  ═══════════════════════════════════════════════════════════   │
│                                                                 │
│  colony-tutorial> name = "Colony"                               │
│                                                                 │
│  ✗ Not quite right                                              │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ ⚠ FEEDBACK                                                │  │
│  │                                                          │  │
│  │ You wrote: name = "Colony"                               │  │
│  │                 ^                                        │  │
│  │                 └─ Extra spaces here                     │  │
│  │                                                          │  │
│  │ Variable assignments can't have spaces around =         │  │
│  │                                                          │  │
│  │ The shell saw this as trying to run a command           │  │
│  │ called "name" with arguments "=" and "Colony"           │  │
│  │                                                          │  │
│  │ ✓ Correct form: name="Colony"                            │  │
│  │                 (no spaces)                              │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  Try again:                                                     │
│                                                                 │
│  colony-tutorial> _                                             │
│                                                                 │
│  ────────────────────────────────────────────────────────────  │
│  [Ctrl+H] Hint  [Tab] Show correct answer                      │
└─────────────────────────────────────────────────────────────────┘
```

### Menu System

```
┌─────────────────────────────────────────────────────────────────┐
│                   Colony Shell Tutorial                         │
│                                                                 │
│  Choose Your Learning Path                                      │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ → 1. I'm new to shells (start from basics)              │  │
│  │     9 lessons  •  ~30 minutes  •  Beginner              │  │
│  │                                                          │  │
│  │   2. I know bash (show me Colony features)              │  │
│  │     7 lessons  •  ~20 minutes  •  Intermediate          │  │
│  │                                                          │  │
│  │   3. I want deep internals (parser, execution)          │  │
│  │     4 lessons  •  ~15 minutes  •  Advanced              │  │
│  │                                                          │  │
│  │   4. Resume from checkpoint                              │  │
│  │     Last: Lesson 4 (Variable Expansion)                 │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ PROGRESS                                                 │  │
│  │                                                          │  │
│  │ Beginner Path:     ███████░░░  70% (7/10)                │  │
│  │ Colony Features:   ███░░░░░░░  30% (2/7)                 │  │
│  │ Internals:         ░░░░░░░░░░   0% (0/4)                 │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  Use arrow keys or numbers to select                            │
│                                                                 │
│  [Enter] Start  [Q] Quit  [H] Help                             │
└─────────────────────────────────────────────────────────────────┘
```

---

## Interaction Patterns

### 1. Input Validation Levels

```rust
enum ValidationResult {
    /// Exactly correct
    Correct,
    /// Functionally equivalent (e.g., 'echo hello' vs 'echo "hello"')
    Equivalent,
    /// Close but needs correction
    AlmostCorrect { feedback: String },
    /// Wrong approach
    Incorrect { feedback: String, hint: Option<String> },
    /// Valid shell syntax but not the lesson goal
    OffTopic,
}
```

### 2. Progressive Hints

```
Hint Level 1 (Ctrl+H once):   "Variables are assigned with name=value"
Hint Level 2 (Ctrl+H twice):  "Try typing: name="
Hint Level 3 (Ctrl+H third):  "The full command is: name=\"Colony\""
Hint Level 4 (Tab):            Auto-fill the correct answer
```

### 3. Magic Reveal Toggle

Users can toggle between:
- **Normal Mode:** Just shows output
- **Magic Mode:** Shows internal state (AST, expansion steps, tuples)

```
┌─────────────────────────────────────────────────────────────────┐
│  Output: Hello, Colony!                                         │
│                                                                 │
│  [Ctrl+M] Show Magic ▼                                          │
└─────────────────────────────────────────────────────────────────┘

       ↓ (user presses Ctrl+M)

┌─────────────────────────────────────────────────────────────────┐
│  Output: Hello, Colony!                                         │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ THE MAGIC ▼                                              │  │
│  │                                                          │  │
│  │ AST: SimpleCommand("echo", ["Hello, Colony!"])          │  │
│  │ Expansion: $name → "Colony"                              │  │
│  │ Execution: fork() → execve("/bin/echo", ...)            │  │
│  │ Exit status: 0                                           │  │
│  │                                                          │  │
│  │ Tuples emitted:                                          │  │
│  │   ("cmd_complete", 0, 15ms)                              │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
│  [Ctrl+M] Hide Magic ▲                                          │
└─────────────────────────────────────────────────────────────────┘
```

### 4. Checkpoint System

```
Auto-save after each completed lesson:
  ~/.config/colony/tutorial-progress.toml

Format:
  [progress]
  last_lesson = "lesson-04-expansion"
  completed = ["lesson-01-intro", "lesson-02-files", "lesson-03-args", "lesson-04-expansion"]
  path = "beginner"
  
  [stats]
  total_time_seconds = 1847
  hints_used = 12
  retries = 3
```

---

## Content Outline

### Lesson 1: What is a Shell?
- **Concept:** The shell is a command interpreter
- **Exercises:**
  1. Type `echo "Hello"`
  2. Type `ls`
  3. Type an invalid command (see error)
- **Magic:** How PATH lookup works

### Lesson 2: Files and Directories
- **Concept:** Filesystem as tree, current working directory
- **Exercises:**
  1. `pwd` (where am I?)
  2. `ls` (what's here?)
  3. `cd /tmp` (go somewhere)
  4. `cd -` (go back)
- **Magic:** CWD stored in ExpansionContext, tuple emission on change

### Lesson 3: Arguments and Options
- **Concept:** Commands take inputs
- **Exercises:**
  1. `echo hello world` (multiple arguments)
  2. `ls -l` (option changes behavior)
  3. `ls -la /etc` (option + argument)
- **Magic:** Argument parsing in SimpleCommand AST

### Lesson 4: Variable Expansion (SAMPLE LESSON - IMPLEMENTED)
- **Concept:** $name becomes value
- **Exercises:**
  1. `name="Colony"`
  2. `echo "Hello, $name!"`
  3. `echo $USER`
- **Magic:** Expansion order, IntermediateWord tracking

### Lesson 5: Pipelines
- **Concept:** Chain commands together
- **Exercises:**
  1. `echo "hello" | wc -c`
  2. `ls | grep .rs`
  3. `ps aux | grep colony | wc -l`
- **Magic:** Pipe creation, stdout → stdin wiring

### Lesson 6: Redirections
- **Concept:** Send output to files
- **Exercises:**
  1. `echo "test" > file.txt`
  2. `cat file.txt`
  3. `echo "more" >> file.txt` (append)
  4. `cat < file.txt`
- **Magic:** File descriptor manipulation (open, dup2)

### Lesson 7: Control Flow (for loop)
- **Concept:** Repeat actions
- **Exercises:**
  1. `for i in 1 2 3; do echo $i; done`
  2. `for file in *.rs; do echo $file; done`
- **Magic:** Loop variable binding, body execution

### Lesson 8: Control Flow (if statement)
- **Concept:** Make decisions
- **Exercises:**
  1. `if [ -f file.txt ]; then echo "exists"; fi`
  2. `if [ $? -eq 0 ]; then echo "success"; fi`
- **Magic:** Condition evaluation, branch selection

### Lesson 9: Functions
- **Concept:** Reusable code blocks
- **Exercises:**
  1. Define: `greet() { echo "Hello, $1!"; }`
  2. Call: `greet Colony`
- **Magic:** Function storage in ExpansionContext

### Lesson 10: Extended Channels (fd 3-6)
- **Concept:** Semantic channels for context/llm/tools/trace
- **Exercises:**
  1. `echo "important context" >&3`
  2. `context show` (see accumulated context)
  3. `echo "trace point" >&6`
- **Magic:** ChannelManager, tuple space sync

### Lesson 11: Reactive Environment
- **Concept:** State changes emit tuples
- **Exercises:**
  1. Enable trace: `set -x`
  2. `cd /tmp` (watch tuple emission)
  3. `export FOO=bar` (watch env_change tuple)
- **Magic:** Tuple space pub/sub, listeners

### Lesson 12: Natural Language Detection
- **Concept:** # or ? prefix triggers agent
- **Exercises:**
  1. `# find all rust files in this directory`
  2. `? what does this command do: awk '{print $2}'`
- **Magic:** Parser fallback → NaturalLanguageDetector → LLM

### Lesson 13: Agent Integration
- **Concept:** Agents as first-class citizens
- **Exercises:**
  1. `agent status`
  2. `llm status`
  3. `llm use ollama:llama2` (switch provider)
- **Magic:** ModelRouter, multi-provider detection

### Lesson 14: Typed Pipelines
- **Concept:** `->` vs `=>` vs `~>`
- **Exercises:**
  1. `find . -name "*.rs" -> count` (typed pipe)
  2. `cat data.json => jq .name` (auto-converting)
- **Magic:** Type annotations, pipeline metadata

### Lesson 15: MCP Tool Integration
- **Concept:** Call external tools with types
- **Exercises:**
  1. `mcp list` (show available servers)
  2. Use agent to call MCP tool
- **Magic:** TypeScript codegen, rustyscript sandbox

### Lesson 16: Project Configuration (.colonyrc)
- **Concept:** Per-project agent personality
- **Exercises:**
  1. Create `.colonyrc`
  2. Add agent personality
  3. Trust the config (direnv-style)
- **Magic:** TrustManager, SHA256 hashing

### Lesson 17: Lexer and Parser
- **Concept:** Text → Tokens → AST
- **Exercises:**
  1. `echo "test $(date +%s)"` (visualize AST)
  2. Complex pipeline visualization
- **Magic:** Pure PEG parser (pest), heredoc preprocessing

### Lesson 18: Expansion Order (Deep)
- **Concept:** 7-step expansion process
- **Exercises:**
  1. Trace expansion of `echo ~/${USER}_$(date +%s).txt`
- **Magic:** IntermediateWord, Expander, quote tracking

### Lesson 19: Process Execution
- **Concept:** fork/exec, process groups
- **Exercises:**
  1. Visualize process tree for pipeline
  2. Job control basics
- **Magic:** nix crate syscalls, SIGCHLD handling

### Lesson 20: Job Control
- **Concept:** fg/bg/jobs, signal handling
- **Exercises:**
  1. Background a job: `sleep 10 &`
  2. `jobs` (list)
  3. `fg %1` (bring to foreground)
- **Magic:** JobTable, process groups, terminal control

---

## Implementation Architecture

### Module Structure

```
crates/colony-shell/src/tutorial/
├── mod.rs              # Module root, public API
├── engine.rs           # Tutorial engine (state machine)
├── lessons/
│   ├── mod.rs          # Lesson registry
│   ├── lesson_01_intro.rs
│   ├── lesson_02_files.rs
│   ├── lesson_03_args.rs
│   ├── lesson_04_expansion.rs  # SAMPLE LESSON (implemented)
│   ├── lesson_05_pipelines.rs
│   └── ...             # Additional lessons
├── ui.rs               # Terminal UI helpers (box-drawing, colors)
├── validation.rs       # Input validation and feedback
├── progress.rs         # Checkpoint saving/loading
└── magic.rs            # "Show the magic" visualization
```

### Key Types

```rust
// crates/colony-shell/src/tutorial/mod.rs

use colony_exec::Executor;
use colony_expand::ExpansionContext;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Tutorial session state
pub struct TutorialSession {
    pub path: LearningPath,
    pub current_lesson: LessonId,
    pub progress: Progress,
    pub context: ExpansionContext,
    pub executor: Executor,
    pub ui_state: UiState,
    pub magic_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LearningPath {
    Beginner,
    BashExpert,
    Internals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub completed: Vec<LessonId>,
    pub current_attempt: usize,
    pub hints_used: usize,
    pub total_time_seconds: u64,
    pub checkpoints: Vec<Checkpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub lesson_id: LessonId,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub terminal_size: (u16, u16), // (width, height)
    pub show_hints: bool,
    pub show_progress_bar: bool,
}

/// Trait that all lessons must implement
pub trait Lesson: Send + Sync {
    /// Unique lesson identifier
    fn id(&self) -> LessonId;
    
    /// Display name
    fn name(&self) -> &str;
    
    /// Brief description (shown in menu)
    fn description(&self) -> &str;
    
    /// Estimated time in minutes
    fn estimated_minutes(&self) -> u8;
    
    /// Difficulty level
    fn difficulty(&self) -> Difficulty;
    
    /// Learning path(s) this lesson belongs to
    fn paths(&self) -> Vec<LearningPath>;
    
    /// Prerequisites (must complete these first)
    fn prerequisites(&self) -> Vec<LessonId> {
        Vec::new()
    }
    
    /// Initialize the lesson (show intro screen)
    fn start(&self, session: &mut TutorialSession) -> Result<(), TutorialError>;
    
    /// Process user input and return validation result
    fn validate_input(
        &self,
        input: &str,
        session: &mut TutorialSession,
    ) -> Result<ValidationResult, TutorialError>;
    
    /// Show the "magic" (internals) for this lesson
    fn show_magic(
        &self,
        session: &TutorialSession,
    ) -> Result<MagicVisualization, TutorialError>;
    
    /// Clean up after lesson (optional)
    fn cleanup(&self, session: &mut TutorialSession) -> Result<(), TutorialError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Beginner,
    Intermediate,
    Advanced,
}

/// Validation result with feedback
#[derive(Debug, Clone)]
pub enum ValidationResult {
    /// Exactly correct - proceed to next step
    Correct {
        output: String,
        next_step: Option<Step>,
    },
    
    /// Functionally equivalent - proceed with note
    Equivalent {
        output: String,
        note: String,
        next_step: Option<Step>,
    },
    
    /// Close but needs correction
    AlmostCorrect {
        feedback: String,
        suggestion: Option<String>,
    },
    
    /// Wrong approach
    Incorrect {
        feedback: String,
        hint: Option<String>,
    },
    
    /// Valid shell syntax but not the lesson goal
    OffTopic {
        message: String,
    },
    
    /// Lesson complete
    Complete {
        summary: String,
        next_lesson: Option<LessonId>,
    },
}

/// Single step in multi-step lesson
#[derive(Debug, Clone)]
pub struct Step {
    pub number: usize,
    pub total_steps: usize,
    pub instruction: String,
    pub expected_pattern: String, // Regex or exact match
    pub hint_levels: Vec<String>,
}

/// Magic visualization data
#[derive(Debug, Clone)]
pub struct MagicVisualization {
    pub sections: Vec<MagicSection>,
}

#[derive(Debug, Clone)]
pub struct MagicSection {
    pub title: String,
    pub content: Vec<String>, // Lines of output
    pub style: MagicStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicStyle {
    Ast,        // Show syntax tree
    Expansion,  // Show expansion steps
    Execution,  // Show process/syscall details
    Tuples,     // Show tuple space activity
    Network,    // Show agent/MCP communication
}

#[derive(Debug, thiserror::Error)]
pub enum TutorialError {
    #[error("Lesson not found: {0}")]
    LessonNotFound(LessonId),
    
    #[error("Prerequisites not met: {0:?}")]
    PrerequisitesNotMet(Vec<LessonId>),
    
    #[error("Execution error: {0}")]
    ExecutionError(#[from] colony_exec::ExecutionError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Progress save error: {0}")]
    ProgressError(String),
}
```

### Integration with Main Binary

```rust
// crates/colony-shell/src/main.rs

use colony_shell::tutorial::{TutorialSession, LearningPath};

fn main() -> Result<()> {
    // Parse command-line arguments
    let opts = ShellOptions::parse();
    
    // Check for --tutorial flag
    if opts.tutorial {
        // Enter tutorial mode
        let path = select_learning_path()?; // Show menu
        let session = TutorialSession::new(path)?;
        return run_tutorial(session).await;
    }
    
    // Normal shell mode...
}

async fn run_tutorial(mut session: TutorialSession) -> Result<()> {
    use colony_shell::tutorial::{LessonRegistry, UiRenderer};
    
    let registry = LessonRegistry::default();
    let mut renderer = UiRenderer::new()?;
    
    loop {
        // Get current lesson
        let lesson = registry.get(&session.current_lesson)
            .ok_or(TutorialError::LessonNotFound(session.current_lesson.clone()))?;
        
        // Start lesson
        lesson.start(&mut session)?;
        renderer.render_lesson_intro(lesson, &session)?;
        
        // REPL for this lesson
        loop {
            // Render prompt
            renderer.render_prompt(&session)?;
            
            // Read input
            let input = renderer.read_line()?;
            
            // Handle special commands
            match input.trim() {
                ":menu" => break, // Return to menu
                ":hint" => {
                    session.progress.hints_used += 1;
                    renderer.render_hint(lesson, &session)?;
                    continue;
                }
                ":skip" => {
                    // Mark as skipped, move to next
                    break;
                }
                ":magic" => {
                    session.magic_mode = !session.magic_mode;
                    renderer.render_magic_toggle(session.magic_mode)?;
                    continue;
                }
                _ => {}
            }
            
            // Validate input
            match lesson.validate_input(&input, &mut session)? {
                ValidationResult::Correct { output, next_step } => {
                    renderer.render_success(&output)?;
                    
                    if session.magic_mode {
                        let magic = lesson.show_magic(&session)?;
                        renderer.render_magic(&magic)?;
                    }
                    
                    if next_step.is_some() {
                        // Multi-step lesson - continue
                        renderer.render_next_step(&next_step.unwrap())?;
                    } else {
                        // Single-step lesson - complete
                        renderer.render_lesson_complete(lesson)?;
                        session.progress.completed.push(lesson.id());
                        session.save_progress()?;
                        break;
                    }
                }
                
                ValidationResult::AlmostCorrect { feedback, suggestion } => {
                    renderer.render_almost_correct(&feedback, suggestion.as_deref())?;
                    session.progress.current_attempt += 1;
                }
                
                ValidationResult::Incorrect { feedback, hint } => {
                    renderer.render_incorrect(&feedback, hint.as_deref())?;
                    session.progress.current_attempt += 1;
                }
                
                ValidationResult::Complete { summary, next_lesson } => {
                    renderer.render_lesson_complete_with_summary(lesson, &summary)?;
                    session.progress.completed.push(lesson.id());
                    session.save_progress()?;
                    
                    if let Some(next_id) = next_lesson {
                        session.current_lesson = next_id;
                    } else {
                        // Path complete
                        renderer.render_path_complete(&session)?;
                        return Ok(());
                    }
                    break;
                }
                
                _ => {
                    // Other validation results handled similarly
                }
            }
        }
        
        // Lesson cleanup
        lesson.cleanup(&mut session)?;
    }
    
    Ok(())
}
```

---

## Sample Lesson Implementation (Lesson 4: Variable Expansion)

```rust
// crates/colony-shell/src/tutorial/lessons/lesson_04_expansion.rs

use crate::tutorial::{
    Lesson, LessonId, LearningPath, Difficulty, TutorialSession,
    ValidationResult, Step, MagicVisualization, MagicSection, MagicStyle,
    TutorialError,
};
use colony_parser::pest_api::parse_statement;
use colony_expand::Expander;
use std::io::Write;

/// Lesson 4: Variable Expansion
pub struct ExpansionLesson {
    current_step: usize,
}

impl Default for ExpansionLesson {
    fn default() -> Self {
        Self { current_step: 0 }
    }
}

impl Lesson for ExpansionLesson {
    fn id(&self) -> LessonId {
        LessonId("lesson-04-expansion".to_string())
    }
    
    fn name(&self) -> &str {
        "Variable Expansion"
    }
    
    fn description(&self) -> &str {
        "Understand how Colony expands variables in commands"
    }
    
    fn estimated_minutes(&self) -> u8 {
        5
    }
    
    fn difficulty(&self) -> Difficulty {
        Difficulty::Beginner
    }
    
    fn paths(&self) -> Vec<LearningPath> {
        vec![LearningPath::Beginner, LearningPath::Internals]
    }
    
    fn prerequisites(&self) -> Vec<LessonId> {
        vec![
            LessonId("lesson-01-intro".to_string()),
            LessonId("lesson-03-args".to_string()),
        ]
    }
    
    fn start(&self, _session: &mut TutorialSession) -> Result<(), TutorialError> {
        // Intro screen is rendered by the tutorial engine
        // This is where we'd do any lesson-specific setup
        Ok(())
    }
    
    fn validate_input(
        &self,
        input: &str,
        session: &mut TutorialSession,
    ) -> Result<ValidationResult, TutorialError> {
        let input = input.trim();
        
        match self.current_step {
            0 => {
                // Step 1: Assign variable
                // Expected: name="Colony"
                
                if input.is_empty() {
                    return Ok(ValidationResult::OffTopic {
                        message: "Type the command to assign a variable.".to_string(),
                    });
                }
                
                // Check if it's an assignment
                if !input.contains('=') {
                    return Ok(ValidationResult::Incorrect {
                        feedback: "This doesn't look like a variable assignment.".to_string(),
                        hint: Some("Use the format: variable_name=value".to_string()),
                    });
                }
                
                // Check for spaces around =
                if input.contains(" = ") {
                    return Ok(ValidationResult::AlmostCorrect {
                        feedback: "Variable assignments can't have spaces around =".to_string(),
                        suggestion: Some("Try: name=\"Colony\" (no spaces)".to_string()),
                    });
                }
                
                // Parse and check if it's the expected assignment
                if let Ok(stmt) = parse_statement(input) {
                    use colony_parser::Statement;
                    
                    if let Statement::SimpleCommand(cmd) = stmt {
                        if cmd.name.as_literal() == Some("name") && cmd.args.is_empty() {
                            // This is trying to execute "name" as a command
                            return Ok(ValidationResult::Incorrect {
                                feedback: "Without the quotes, the shell tries to run 'name' as a command.".to_string(),
                                hint: Some("Add quotes: name=\"Colony\"".to_string()),
                            });
                        }
                        
                        // Check for assignment in SimpleCommand
                        if let Some(assignment) = cmd.assignments.first() {
                            if assignment.name == "name" {
                                // Extract the value
                                let value = assignment.value.as_ref()
                                    .and_then(|w| w.as_literal())
                                    .unwrap_or("");
                                
                                if value.is_empty() {
                                    return Ok(ValidationResult::AlmostCorrect {
                                        feedback: "The variable needs a value.".to_string(),
                                        suggestion: Some("Try: name=\"Colony\"".to_string()),
                                    });
                                }
                                
                                // Accept any value, but note if different
                                session.context.set_var("name".to_string(), value.to_string());
                                
                                let output = format!("✓ Variable assigned: name=\"{}\"", value);
                                
                                if value != "Colony" {
                                    return Ok(ValidationResult::Equivalent {
                                        output,
                                        note: format!("You used \"{}\", but the example shows \"Colony\". Either works!", value),
                                        next_step: Some(Step {
                                            number: 2,
                                            total_steps: 2,
                                            instruction: "Now use the variable in a command:\n  echo \"Hello, $name!\"".to_string(),
                                            expected_pattern: r#"echo\s+".*\$name.*""#.to_string(),
                                            hint_levels: vec![
                                                "Use echo with the $name variable".to_string(),
                                                "Try: echo \"Hello, $name!\"".to_string(),
                                            ],
                                        }),
                                    });
                                }
                                
                                return Ok(ValidationResult::Correct {
                                    output,
                                    next_step: Some(Step {
                                        number: 2,
                                        total_steps: 2,
                                        instruction: "Now use the variable in a command:\n  echo \"Hello, $name!\"".to_string(),
                                        expected_pattern: r#"echo\s+".*\$name.*""#.to_string(),
                                        hint_levels: vec![
                                            "Use echo with the $name variable".to_string(),
                                            "Variables are referenced with $".to_string(),
                                            "Try: echo \"Hello, $name!\"".to_string(),
                                        ],
                                    }),
                                });
                            }
                        }
                    }
                }
                
                Ok(ValidationResult::Incorrect {
                    feedback: "That doesn't match the expected format.".to_string(),
                    hint: Some("Try: name=\"Colony\"".to_string()),
                })
            }
            
            1 => {
                // Step 2: Use variable in echo command
                // Expected: echo "Hello, $name!"
                
                if !input.contains("echo") {
                    return Ok(ValidationResult::OffTopic {
                        message: "This step requires using the echo command.".to_string(),
                    });
                }
                
                if !input.contains('$') {
                    return Ok(ValidationResult::Incorrect {
                        feedback: "You need to reference the variable using $.".to_string(),
                        hint: Some("Use $name to reference the variable".to_string()),
                    });
                }
                
                // Execute the command
                if let Ok(stmt) = parse_statement(input) {
                    // Execute and capture output
                    let output = match session.executor.execute_statement(&stmt).await {
                        Ok(status) => {
                            if status.code() == 0 {
                                // Get the actual output (simplified - would need to capture stdout)
                                let name_value = session.context.get_var_or_env("name")
                                    .unwrap_or_else(|| "".to_string());
                                
                                let expected_output = format!("Hello, {}!", name_value);
                                
                                // Show success with magic
                                return Ok(ValidationResult::Complete {
                                    summary: format!(
                                        "Great work! You've learned how variable expansion works.\n\n\
                                         The shell replaced $name with \"{}\" before executing the command.\n\n\
                                         Output: {}", 
                                        name_value, expected_output
                                    ),
                                    next_lesson: Some(LessonId("lesson-05-pipelines".to_string())),
                                });
                            } else {
                                "Command failed".to_string()
                            }
                        }
                        Err(e) => format!("Error: {}", e),
                    };
                    
                    return Ok(ValidationResult::Incorrect {
                        feedback: output,
                        hint: Some("Make sure the variable exists and the syntax is correct".to_string()),
                    });
                }
                
                Ok(ValidationResult::Incorrect {
                    feedback: "Couldn't parse that command.".to_string(),
                    hint: Some("Try: echo \"Hello, $name!\"".to_string()),
                })
            }
            
            _ => Ok(ValidationResult::Complete {
                summary: "Lesson complete!".to_string(),
                next_lesson: Some(LessonId("lesson-05-pipelines".to_string())),
            }),
        }
    }
    
    fn show_magic(
        &self,
        session: &TutorialSession,
    ) -> Result<MagicVisualization, TutorialError> {
        // Build visualization of what happened
        let mut sections = Vec::new();
        
        // Section 1: AST
        sections.push(MagicSection {
            title: "Step 1: PARSE".to_string(),
            content: vec![
                "echo \"Hello, $name!\"".to_string(),
                "───▶ SimpleCommand {".to_string(),
                "       name: \"echo\",".to_string(),
                "       args: [Word([Literal(\"Hello, \"),".to_string(),
                "                   Dollar(\"name\"),".to_string(),
                "                   Literal(\"!\")])]".to_string(),
                "     }".to_string(),
            ],
            style: MagicStyle::Ast,
        });
        
        // Section 2: Expansion
        let name_value = session.context.get_var_or_env("name")
            .unwrap_or_else(|| "".to_string());
        
        sections.push(MagicSection {
            title: "Step 2: EXPAND".to_string(),
            content: vec![
                "\"Hello, $name!\"".to_string(),
                "        ╰─────╯".to_string(),
                "           │".to_string(),
                "           ▼".to_string(),
                format!("\"Hello, {}!\"    (lookup in environment)", name_value),
            ],
            style: MagicStyle::Expansion,
        });
        
        // Section 3: Execution
        sections.push(MagicSection {
            title: "Step 3: EXECUTE".to_string(),
            content: vec![
                format!("/bin/echo \"Hello, {}!\"", name_value),
                "".to_string(),
                format!("Output: Hello, {}!", name_value),
            ],
            style: MagicStyle::Execution,
        });
        
        Ok(MagicVisualization { sections })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lesson_metadata() {
        let lesson = ExpansionLesson::default();
        assert_eq!(lesson.name(), "Variable Expansion");
        assert_eq!(lesson.estimated_minutes(), 5);
        assert_eq!(lesson.difficulty(), Difficulty::Beginner);
    }
    
    // Additional tests...
}
```

---

## Terminal UI Helpers

```rust
// crates/colony-shell/src/tutorial/ui.rs

use crossterm::{
    cursor, execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor, Stylize},
    terminal::{self, Clear, ClearType},
};
use std::io::{self, Write};

/// Box-drawing characters (Unicode)
pub mod box_chars {
    pub const HORIZONTAL: char = '─';
    pub const VERTICAL: char = '│';
    pub const TOP_LEFT: char = '┌';
    pub const TOP_RIGHT: char = '┐';
    pub const BOTTOM_LEFT: char = '└';
    pub const BOTTOM_RIGHT: char = '┘';
    pub const T_DOWN: char = '┬';
    pub const T_UP: char = '┴';
    pub const T_RIGHT: char = '├';
    pub const T_LEFT: char = '┤';
    pub const CROSS: char = '┼';
}

/// Terminal UI renderer for tutorial mode
pub struct UiRenderer {
    stdout: io::Stdout,
    width: u16,
    height: u16,
}

impl UiRenderer {
    pub fn new() -> io::Result<Self> {
        let (width, height) = terminal::size()?;
        Ok(Self {
            stdout: io::stdout(),
            width,
            height,
        })
    }
    
    /// Draw a box with title
    pub fn draw_box(&mut self, title: &str, content: &[String], width: usize) -> io::Result<()> {
        use box_chars::*;
        
        // Top border with title
        write!(self.stdout, "{}", TOP_LEFT)?;
        if !title.is_empty() {
            write!(self.stdout, " {} ", title)?;
            let remaining = width.saturating_sub(title.len() + 4);
            write!(self.stdout, "{}", HORIZONTAL.to_string().repeat(remaining))?;
        } else {
            write!(self.stdout, "{}", HORIZONTAL.to_string().repeat(width - 2))?;
        }
        writeln!(self.stdout, "{}", TOP_RIGHT)?;
        
        // Content lines
        for line in content {
            writeln!(self.stdout, "{} {:width$} {}", 
                VERTICAL, line, VERTICAL, width = width - 2)?;
        }
        
        // Bottom border
        write!(self.stdout, "{}", BOTTOM_LEFT)?;
        write!(self.stdout, "{}", HORIZONTAL.to_string().repeat(width - 2))?;
        writeln!(self.stdout, "{}", BOTTOM_RIGHT)?;
        
        self.stdout.flush()
    }
    
    /// Draw section separator
    pub fn draw_separator(&mut self, width: usize) -> io::Result<()> {
        writeln!(self.stdout, "{}", "─".repeat(width))?;
        self.stdout.flush()
    }
    
    /// Render colored text
    pub fn print_colored(&mut self, text: &str, color: Color) -> io::Result<()> {
        queue!(
            self.stdout,
            SetForegroundColor(color),
            Print(text),
            ResetColor
        )?;
        self.stdout.flush()
    }
    
    /// Render success message
    pub fn render_success(&mut self, message: &str) -> io::Result<()> {
        queue!(
            self.stdout,
            SetForegroundColor(Color::Green),
            Print("✓ "),
            Print(message),
            ResetColor,
            Print("\n")
        )?;
        self.stdout.flush()
    }
    
    /// Render error message
    pub fn render_error(&mut self, message: &str) -> io::Result<()> {
        queue!(
            self.stdout,
            SetForegroundColor(Color::Red),
            Print("✗ "),
            Print(message),
            ResetColor,
            Print("\n")
        )?;
        self.stdout.flush()
    }
    
    /// Render warning message
    pub fn render_warning(&mut self, message: &str) -> io::Result<()> {
        queue!(
            self.stdout,
            SetForegroundColor(Color::Yellow),
            Print("⚠ "),
            Print(message),
            ResetColor,
            Print("\n")
        )?;
        self.stdout.flush()
    }
    
    /// Render hint
    pub fn render_hint(&mut self, hint: &str) -> io::Result<()> {
        queue!(
            self.stdout,
            SetForegroundColor(Color::Cyan),
            Print("💡 HINT: "),
            ResetColor,
            Print(hint),
            Print("\n")
        )?;
        self.stdout.flush()
    }
    
    /// Clear screen and move cursor to top
    pub fn clear_screen(&mut self) -> io::Result<()> {
        execute!(
            self.stdout,
            Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )
    }
    
    /// Render prompt for user input
    pub fn render_prompt(&mut self, prompt: &str) -> io::Result<()> {
        queue!(
            self.stdout,
            SetForegroundColor(Color::Blue),
            Print(prompt),
            ResetColor,
            Print(" ")
        )?;
        self.stdout.flush()
    }
    
    /// Read line from stdin
    pub fn read_line(&mut self) -> io::Result<String> {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(input)
    }
    
    /// Update terminal size (call on SIGWINCH)
    pub fn update_size(&mut self) -> io::Result<()> {
        let (width, height) = terminal::size()?;
        self.width = width;
        self.height = height;
        Ok(())
    }
    
    /// Get current terminal dimensions
    pub fn dimensions(&self) -> (u16, u16) {
        (self.width, self.height)
    }
}

/// Helper to center text in a given width
pub fn center_text(text: &str, width: usize) -> String {
    let text_len = text.len();
    if text_len >= width {
        return text.to_string();
    }
    
    let padding = (width - text_len) / 2;
    format!("{}{}", " ".repeat(padding), text)
}

/// Helper to truncate text with ellipsis
pub fn truncate(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    
    format!("{}...", &text[..max_len.saturating_sub(3)])
}
```

---

## Progress Tracking

```rust
// crates/colony-shell/src/tutorial/progress.rs

use crate::tutorial::{Progress, LessonId, Checkpoint};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct ProgressFile {
    version: u32,
    last_lesson: String,
    completed: Vec<String>,
    path: String,
    stats: ProgressStats,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProgressStats {
    total_time_seconds: u64,
    hints_used: usize,
    retries: usize,
}

impl Progress {
    /// Save progress to disk
    pub fn save(&self, path_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let progress_file = Self::progress_file_path()?;
        
        let file_data = ProgressFile {
            version: 1,
            last_lesson: self.checkpoints
                .last()
                .map(|c| c.lesson_id.0.clone())
                .unwrap_or_default(),
            completed: self.completed.iter().map(|id| id.0.clone()).collect(),
            path: path_name.to_string(),
            stats: ProgressStats {
                total_time_seconds: self.total_time_seconds,
                hints_used: self.hints_used,
                retries: self.current_attempt,
            },
        };
        
        let toml = toml::to_string_pretty(&file_data)?;
        
        // Ensure parent directory exists
        if let Some(parent) = progress_file.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(&progress_file, toml)?;
        
        Ok(())
    }
    
    /// Load progress from disk
    pub fn load() -> Result<(Self, String), Box<dyn std::error::Error>> {
        let progress_file = Self::progress_file_path()?;
        
        if !progress_file.exists() {
            return Ok((Self::default(), String::new()));
        }
        
        let toml_str = fs::read_to_string(&progress_file)?;
        let file_data: ProgressFile = toml::from_str(&toml_str)?;
        
        let progress = Progress {
            completed: file_data.completed.into_iter().map(LessonId).collect(),
            current_attempt: file_data.stats.retries,
            hints_used: file_data.stats.hints_used,
            total_time_seconds: file_data.stats.total_time_seconds,
            checkpoints: Vec::new(), // Reconstructed from completed
        };
        
        Ok((progress, file_data.path))
    }
    
    fn progress_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not determine config directory")?;
        
        Ok(config_dir.join("colony").join("tutorial-progress.toml"))
    }
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            completed: Vec::new(),
            current_attempt: 0,
            hints_used: 0,
            total_time_seconds: 0,
            checkpoints: Vec::new(),
        }
    }
}
```

---

## Phased Implementation Roadmap

### Phase 1: Infrastructure (Week 1)
- [x] Create `crates/colony-shell/src/tutorial/` module structure
- [x] Implement `TutorialSession` and core types
- [x] Implement `Lesson` trait
- [x] Implement `UiRenderer` with box-drawing helpers
- [x] Implement `Progress` save/load

### Phase 2: Sample Lesson (Week 1)
- [x] Implement Lesson 4: Variable Expansion (complete)
- [x] Test full user flow (start → input → validation → magic → complete)
- [x] Polish UI rendering

### Phase 3: Menu & Navigation (Week 2)
- [ ] Implement learning path selection screen
- [ ] Implement lesson list/menu
- [ ] Implement checkpoint resume
- [ ] Add progress tracking UI

### Phase 4: Beginner Path (Week 2-3)
- [ ] Lesson 1: What is a Shell?
- [ ] Lesson 2: Files and Directories
- [ ] Lesson 3: Arguments and Options
- [ ] Lesson 5: Pipelines
- [ ] Lesson 6: Redirections
- [ ] Lesson 7: Control Flow (for)
- [ ] Lesson 8: Control Flow (if)
- [ ] Lesson 9: Functions

### Phase 5: Colony Features Path (Week 3-4)
- [ ] Lesson 10: Extended Channels
- [ ] Lesson 11: Reactive Environment
- [ ] Lesson 12: Natural Language Detection
- [ ] Lesson 13: Agent Integration
- [ ] Lesson 14: Typed Pipelines
- [ ] Lesson 15: MCP Tool Integration
- [ ] Lesson 16: Project Configuration

### Phase 6: Internals Path (Week 4)
- [ ] Lesson 17: Lexer and Parser
- [ ] Lesson 18: Expansion Order
- [ ] Lesson 19: Process Execution
- [ ] Lesson 20: Job Control

### Phase 7: Polish & Testing (Week 5)
- [ ] Comprehensive testing
- [ ] Error handling improvements
- [ ] Accessibility (screen readers, colorblind-friendly)
- [ ] Performance optimization
- [ ] Documentation

---

## Key Design Decisions

### 1. Terminal-Native vs Web UI
**Decision:** Terminal-native only (at least initially)

**Rationale:**
- Aligns with shell identity
- No dependencies on browsers
- Works over SSH
- Fast iteration

**Future:** Could add web version for marketing/demos

### 2. Sandbox vs Real Shell
**Decision:** Real shell with safety guardrails

**Rationale:**
- Users see actual behavior
- Builds real muscle memory
- Easy to transition to normal use

**Safety:**
- Lessons don't run destructive commands
- Working directory is temp space
- Can't modify system files

### 3. AST Visualization Depth
**Decision:** Simplified AST for beginners, full AST for internals path

**Rationale:**
- Don't overwhelm beginners with Rust syntax
- Advanced users want full details

**Implementation:**
- Lesson trait has `show_magic()` method
- Can customize per lesson

### 4. Progress Persistence
**Decision:** Save to TOML file in config directory

**Rationale:**
- Simple, human-readable
- Easy to edit/debug
- No database dependencies

**Location:** `~/.config/colony/tutorial-progress.toml`

---

## Success Metrics

### Completion Rates
- **Target:** >60% of users complete at least 3 lessons
- **Measure:** Track lesson completions in telemetry (opt-in)

### Time to Proficiency
- **Target:** Users can write basic pipelines within 15 minutes
- **Measure:** Time from tutorial start to Lesson 5 complete

### Hint Usage
- **Target:** <30% of users need hints on any given lesson
- **Measure:** Hints requested per lesson

### Satisfaction
- **Target:** >80% "helpful" rating
- **Measure:** Optional feedback at end of each lesson

---

## Future Enhancements

### 1. Agent-Assisted Hints
- Use LLM to generate contextual hints based on user's mistake
- "The agent noticed you forgot quotes — would you like an explanation?"

### 2. Custom Lessons
- Allow users to create lessons for project-specific workflows
- Share lessons with team via `.colony/tutorials/`

### 3. Interactive Challenges
- Timed challenges (speed typing)
- Puzzle mode (achieve goal with minimal commands)
- Leaderboards (opt-in)

### 4. Video Recordings
- Record terminal sessions for lessons
- Playback with pause/scrub
- Export to asciinema format

### 5. Accessibility
- Screen reader support
- High-contrast themes
- Keyboard-only navigation
- Configurable fonts/sizes

---

## Conclusion

Tutorial mode transforms Colony Shell from "just another shell" into a **teaching tool** that builds deep understanding through progressive, hands-on learning. By making the invisible visible and rewarding curiosity, we create users who don't just memorize commands — they **understand the system**.

The sample implementation (Lesson 4: Variable Expansion) demonstrates the full UX flow and serves as a template for all future lessons. The phased roadmap allows for iterative development and early user feedback.

**Next Steps:**
1. Implement Phase 1 infrastructure
2. Build and test sample lesson
3. Gather feedback from beta users
4. Iterate on UX based on real usage
5. Expand lesson library

---

**Document Status:** Ready for Implementation  
**Review Required:** UX design, accessibility considerations  
**Dependencies:** crossterm (terminal UI), toml (progress persistence)
