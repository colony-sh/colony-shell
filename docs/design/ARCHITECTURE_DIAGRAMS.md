# Colony Shell Architecture Diagrams

This document provides visual diagrams showing how the different pieces of colony-shell fit together at different levels of abstraction.

## Table of Contents

1. [Level 1: High-Level System Overview](#level-1-high-level-system-overview)
2. [Level 2: Crate Dependency Graph](#level-2-crate-dependency-graph)
3. [Level 3: Input Processing Flow](#level-3-input-processing-flow)
4. [Level 4: Execution Engine Architecture](#level-4-execution-engine-architecture)
5. [Level 5: POSIX Expansion System](#level-5-posix-expansion-system)
6. [Level 6: Agent System Architecture](#level-6-agent-system-architecture)
7. [Level 7: Channel & Coordination Systems](#level-7-channel--coordination-systems)
8. [Level 8: Complete Data Flow](#level-8-complete-data-flow)
9. [Summary Table](#summary-table-colony-shell-components)

---

## Level 1: High-Level System Overview

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              COLONY SHELL SYSTEM                                     │
│                    "AI-Native Shell with POSIX Compatibility"                        │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                      │
│    ┌─────────────────────┐          ┌─────────────────────┐                         │
│    │     USER INPUT      │          │   COLONY TERMINAL   │                         │
│    │  (REPL / Script)    │          │    (GPUI - Future)  │                         │
│    └──────────┬──────────┘          └──────────▲──────────┘                         │
│               │                                 │                                    │
│               │ stdin                           │ OSC 2337                           │
│               ▼                                 │ Protocol                           │
│    ┌──────────────────────────────────────────────────────────────────────────┐     │
│    │                         COLONY SHELL BINARY                              │     │
│    │                                                                          │     │
│    │   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐               │     │
│    │   │   Parser    │────▶│  Executor   │────▶│   Output    │               │     │
│    │   │  (Pest PEG) │     │  (Fork/Exec)│     │  (Channels) │               │     │
│    │   └─────────────┘     └──────┬──────┘     └─────────────┘               │     │
│    │                              │                                           │     │
│    │   ┌─────────────┐            │            ┌─────────────┐               │     │
│    │   │   Agent     │◀───────────┼───────────▶│ Tuple Space │               │     │
│    │   │  (LLM API)  │            │            │ (Linda IPC) │               │     │
│    │   └─────────────┘     ┌──────▼──────┐     └─────────────┘               │     │
│    │                       │   Expander   │                                   │     │
│    │                       │   (POSIX)    │                                   │     │
│    │                       └─────────────┘                                   │     │
│    └──────────────────────────────────────────────────────────────────────────┘     │
│                                                                                      │
│    ┌─────────────────────┐     ┌─────────────────────┐     ┌─────────────────────┐  │
│    │   External CMDs     │     │   Builtin CMDs      │     │    AI Providers     │  │
│    │   (PATH lookup)     │     │ (cd, export, echo)  │     │ (Claude/GPT/Ollama) │  │
│    └─────────────────────┘     └─────────────────────┘     └─────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Key Points:**
- Colony Shell is an AI-native shell that maintains full POSIX compatibility
- Two main paths: shell commands (Parser → Executor) and natural language (Agent → LLM)
- Tuple Space provides reactive coordination between components
- Extended channels (fd 3-6) enable semantic I/O beyond traditional stdio

---

## Level 2: Crate Dependency Graph

```
                           ┌─────────────────────┐
                           │   colony-shell      │  ◄── Main Binary (REPL)
                           │   (Entry Point)     │
                           └─────────┬───────────┘
                                     │
            ┌────────────────────────┼────────────────────────┐
            │                        │                        │
            ▼                        ▼                        ▼
   ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
   │  colony-exec    │    │  colony-agent   │    │  colony-config  │
   │  (Execution)    │    │  (AI Agents)    │    │ (Configuration) │
   └────────┬────────┘    └────────┬────────┘    └─────────────────┘
            │                      │
   ┌────────┼──────────────────────┼────────┐
   │        │                      │        │
   ▼        ▼                      ▼        ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│colony-expand │  │colony-channels│ │colony-protocol│ │colony-memory │
│ (POSIX Exp)  │  │ (fd 3-6)     │ │ (OSC 2337)   │  │ (LanceDB+SQL)│
└──────┬───────┘  └──────────────┘ └──────────────┘  └──────────────┘
       │
       ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│colony-parser │  │ colony-tuple │  │colony-tree-  │  │  colony-mcp  │
│ (AST/Grammar)│  │ (Linda Space)│  │   sitter     │  │  (MCP Pool)  │
└──────────────┘  └──────────────┘  │ (Editor CST) │  └──────────────┘
                                    └──────────────┘
       ▲
       │
   FOUNDATION
   (No internal deps)
```

### Dependency Matrix

| Crate | Depends On (Internal) | Depended On By |
|-------|----------------------|----------------|
| **colony-parser** | None | expand, exec, agent, shell |
| **colony-protocol** | None | agent, shell |
| **colony-tuple** | None | exec, agent, mcp, shell |
| **colony-channels** | None | exec |
| **colony-memory** | None | agent (opt), focus |
| **colony-config** | None | shell |
| **colony-tree-sitter** | None | shell |
| **colony-expand** | parser | exec, shell |
| **colony-focus** | memory | (none yet) |
| **colony-mcp** | tuple | (none yet) |
| **colony-agent** | parser, protocol, memory (opt), tuple (opt) | shell |
| **colony-exec** | parser, expand, tuple, channels | shell |
| **colony-shell** | parser, expand, exec, protocol, agent, config, tree-sitter, tuple | (binary) |

---

## Level 3: Input Processing Flow

```
                              User Input
                                  │
                                  ▼
                    ┌─────────────────────────┐
                    │   Tree-Sitter Validate  │  ◄── Incremental, Error-tolerant
                    │   (Syntax Check)        │
                    └─────────────┬───────────┘
                                  │
                         Valid? ──┼── Incomplete?
                           │      │       │
                           │      │       └──▶ Continue Input (Multi-line)
                           │      │
                           ▼      ▼
                    ┌─────────────────────────┐
                    │  Natural Language       │
                    │  Detector               │
                    │                         │
                    │  • Starts with # or ?   │
                    │  • Parse failure        │
                    │  • NL patterns          │
                    └─────────────┬───────────┘
                                  │
                    ┌─────────────┴─────────────┐
                    │                           │
                    ▼                           ▼
         ┌──────────────────┐       ┌──────────────────┐
         │   SHELL PATH     │       │   AGENT PATH     │
         │                  │       │                  │
         │ • Parse (Pest)   │       │ • Build request  │
         │ • Execute        │       │ • Stream LLM     │
         │ • Return status  │       │ • Handle tools   │
         └──────────────────┘       └──────────────────┘
```

### Shell Path Detail

```
    Input: "echo hello | grep h > out.txt && ls"
              │
              ▼
    ┌─────────────────────────────────────────────────────────────────────┐
    │                    PEST PARSER (Pure PEG)                           │
    │                                                                     │
    │  Source ──▶ [Heredoc Preprocess?] ──▶ Pest Parse ──▶ AST Builder   │
    │                                                                     │
    │  AST Output:                                                        │
    │  List {                                                             │
    │    first: Pipeline {                                                │
    │      elements: [                                                    │
    │        SimpleCommand { name: "echo", args: ["hello"], redirects: [] │
    │        SimpleCommand { name: "grep", args: ["h"],                   │
    │                        redirects: [Redirect { op: >, target: "out"}]│
    │      ]                                                              │
    │    },                                                               │
    │    rest: [(And, SimpleCommand { name: "ls" })]                      │
    │  }                                                                  │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## Level 4: Execution Engine Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    EXECUTOR (colony-exec/src/lib.rs)                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  struct Executor {                                                          │
│      context: ExpansionContext,    // Variables, env, aliases, functions   │
│      jobs: JobTable,               // Background jobs, process groups       │
│      tuple_space: TupleSpace,      // Reactive coordination                 │
│      channels: ChannelManager,     // Extended I/O (fd 3-6)                │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
                                     │ execute_simple_command()
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         COMMAND DISPATCH                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                                     │
     ┌───────────────┬───────────────┼───────────────┬───────────────┐
     │               │               │               │               │
     ▼               ▼               ▼               ▼               ▼
┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────────┐
│Function?│   │ Alias?  │   │Builtin? │   │External │   │ Not Found   │
│         │   │         │   │         │   │ Command │   │             │
│ Execute │   │ Expand  │   │ In-Proc │   │         │   │   Error     │
│ in-proc │   │ recurse │   │ execute │   │Fork/Exec│   │             │
└─────────┘   └─────────┘   └────┬────┘   └────┬────┘   └─────────────┘
                                 │              │
                                 ▼              ▼
              ┌──────────────────────────────────────────────────────┐
              │              EXECUTION DETAIL                        │
              ├──────────────────────┬───────────────────────────────┤
              │                      │                               │
              │     BUILTINS         │     EXTERNAL COMMANDS         │
              │    (In-Process)      │      (Fork + Exec)            │
              │                      │                               │
              │ 1. Save current FDs  │ 1. Expand arguments           │
              │ 2. Apply redirects   │ 2. Resolve PATH               │
              │ 3. Execute function  │ 3. Setup channel pipes        │
              │ 4. Emit tuples       │ 4. Fork process               │
              │ 5. Restore FDs       │ 5. In child:                  │
              │                      │    - Apply redirects          │
              │ cd, export, echo,    │    - execve(program)          │
              │ pwd, alias, [, read  │ 6. In parent:                 │
              │ context, llm, etc.   │    - Wait for child           │
              │                      │    - Process channel pipes    │
              │                      │    - Return exit status       │
              └──────────────────────┴───────────────────────────────┘
```

### Pipeline Execution

```
    cmd1 | cmd2 | cmd3
      │     │     │
      └─────┴─────┘
            │
    ┌───────▼───────┐
    │ Create N-1    │      pipe[0]: cmd1.stdout → cmd2.stdin
    │ pipes for N   │      pipe[1]: cmd2.stdout → cmd3.stdin
    │ commands      │
    └───────┬───────┘
            │
    ┌───────▼───────┐
    │ Fork ALL      │      All processes run CONCURRENTLY
    │ processes     │      (not sequential)
    │ concurrently  │
    └───────┬───────┘
            │
    ┌───────▼───────┐
    │ Parent closes │      Prevents deadlock
    │ all pipe ends │      (children own their ends)
    └───────┬───────┘
            │
    ┌───────▼───────┐
    │ Wait for all  │      Return LAST command's exit status
    │ children      │      (POSIX semantics)
    └───────────────┘
```

---

## Level 5: POSIX Expansion System

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    EXPANSION ORDER (POSIX)                                  │
│                    colony-expand/src/lib.rs                                 │
└─────────────────────────────────────────────────────────────────────────────┘

    Input Word: ~user/path/${VAR:-default}/$(echo test)/*.txt
                    │
    ┌───────────────┴───────────────────────────────────────────────────────┐
    │                                                                       │
    │   1. TILDE EXPANSION        ~  →  /home/user                         │
    │      (tilde.rs)             ~+ →  $PWD                               │
    │                             ~- →  $OLDPWD                            │
    │                                                                       │
    │   2. PARAMETER EXPANSION    $VAR        →  value                     │
    │      (parameter.rs)         ${VAR}      →  value                     │
    │                             ${VAR:-def} →  value or "def"            │
    │                             ${VAR:=def} →  assign if unset           │
    │                             ${#VAR}     →  length                    │
    │                             ${VAR%pat}  →  strip suffix              │
    │                             ${arr[@]}   →  all elements              │
    │                                                                       │
    │   3. COMMAND SUBSTITUTION   $(cmd)  →  stdout of cmd                 │
    │      (command.rs)           `cmd`   →  (deprecated form)             │
    │                             ↓                                         │
    │                        ┌────────────────────────────────────┐         │
    │                        │  RECURSIVE: Fork → Exec → Capture │         │
    │                        │  Can contain nested substitutions │         │
    │                        └────────────────────────────────────┘         │
    │                                                                       │
    │   4. ARITHMETIC EXPANSION   $((1+2))    →  3                         │
    │      (arithmetic.rs)        $((a*b))    →  product                   │
    │                             $(($x+1))   →  increment                 │
    │                                                                       │
    │   5. FIELD SPLITTING        "hello world"  →  1 field (quoted)       │
    │      (field_split.rs)       $var           →  may split (IFS)        │
    │                             ${arr[@]}      →  N fields               │
    │                                                                       │
    │   6. PATHNAME EXPANSION     *.txt     →  file1.txt file2.txt         │
    │      (pathname.rs)          ???       →  abc def                     │
    │                             [a-z]*    →  all starting with a-z       │
    │                                                                       │
    │   7. QUOTE REMOVAL          "text"    →  text                        │
    │      (implicit)             'text'    →  text                        │
    │                             \char     →  char                        │
    │                                                                       │
    └───────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
    Output: Vec<Field> = ["file1.txt", "file2.txt", ...]
```

### Key Data Structures

```
    IntermediateWord          Field
    ┌───────────────────┐     ┌───────────────────┐
    │ segments: Vec     │     │ text: String      │
    │   - text: String  │ ──▶ │ quoted: bool      │
    │   - quoted: bool  │     │                   │
    └───────────────────┘     └───────────────────┘
    (tracks quote context      (result after
     through expansion)         splitting)
```

---

## Level 6: Agent System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      AGENT ARCHITECTURE                                     │
│                      colony-agent crate                                     │
└─────────────────────────────────────────────────────────────────────────────┘

                              User Query
                           "# explain this error"
                                  │
                                  ▼
                    ┌─────────────────────────┐
                    │  NaturalLanguageDetector│
                    │                         │
                    │  Triggers:              │
                    │  • # or ? prefix        │
                    │  • Parse failure        │
                    │  • Question patterns    │
                    └─────────────┬───────────┘
                                  │
                                  ▼
                    ┌─────────────────────────┐
                    │        Agent            │
                    │                         │
                    │  • router: ModelRouter  │
                    │  • messages: Vec<Msg>   │
                    │  • tools: ToolRegistry  │
                    │  • config: AgentConfig  │
                    └─────────────┬───────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         MODEL ROUTER                                        │
│                    (Multi-Provider Abstraction)                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Auto-Detection Order:                                                      │
│                                                                             │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                  │
│  │  Anthropic   │───▶│   OpenAI     │───▶│   Ollama     │                  │
│  │  (Claude)    │    │   (GPT-4)    │    │   (Local)    │                  │
│  │              │    │              │    │              │                  │
│  │ ANTHROPIC_   │    │ OPENAI_      │    │ Always       │                  │
│  │ API_KEY      │    │ API_KEY      │    │ Available    │                  │
│  │ required     │    │ required     │    │ (FREE!)      │                  │
│  └──────────────┘    └──────────────┘    └──────────────┘                  │
│                                                                             │
│  All providers implement: ModelProvider trait                               │
│  • complete(request) → CompletionStream                                    │
│  • supports_streaming() → bool                                             │
│  • health_check() → ProviderHealth                                         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  │ Streaming Response
                                  ▼
                    ┌─────────────────────────┐
                    │    Stream Events        │
                    │                         │
                    │  • TextDelta ──────────▶ Print to stdout
                    │  • ToolCallStart ──────▶ Prepare tool
                    │  • ToolCallComplete ───▶ Execute tool
                    │  • Done ───────────────▶ Return response
                    └─────────────────────────┘
                                  │
                         Tool Call?
                    ┌─────────────┴─────────────┐
                    │                           │
                    ▼                           ▼
        ┌──────────────────┐       ┌──────────────────┐
        │   PROGRAMMATIC   │       │   USER-FACING    │
        │   (Auto-execute) │       │ (Needs Approval) │
        │                  │       │                  │
        │  • read_file     │       │  • write_file    │
        │  • search        │       │  • delete_file   │
        │  • list_dir      │       │  • execute_cmd   │
        └──────────────────┘       └────────┬─────────┘
                                            │
                                            ▼
                               ┌──────────────────────┐
                               │  OSC 2337 Widget     │
                               │  (Approval Request)  │
                               │                      │
                               │  Terminal renders    │
                               │  approval dialog     │
                               └──────────────────────┘
```

---

## Level 7: Channel & Coordination Systems

### Extended Channel System

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    FILE DESCRIPTOR LAYOUT                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   fd 0: stdin        ──────────▶  Traditional input                        │
│   fd 1: stdout       ──────────▶  Traditional output                       │
│   fd 2: stderr       ──────────▶  Traditional errors                       │
│   ─────────────────────────────────────────────────────────────────────    │
│   fd 3: stdcontext   ──────────▶  Session context (observations,           │
│                                    decisions, actions)                      │
│   fd 4: stdllm       ──────────▶  LLM conversation stream                  │
│   fd 5: stdtool      ──────────▶  Tool invocations & results               │
│   fd 6: stddiag     ──────────▶  Diagnostics (debug)                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Child Process → Channel Flow

```
    ┌──────────────┐
    │ Child Process│
    │              │
    │ echo "note"  │
    │    >&3       │──────┐
    └──────────────┘      │
                          │ Write to fd 3
                          ▼
    ┌──────────────────────────────────────────────────────────────────────┐
    │                     PIPE CAPTURE (Parent)                            │
    │                                                                      │
    │  1. prepare_channel_redirects() ─────▶ Create pipe for fd 3          │
    │  2. apply_channel_redirects_in_child() ─▶ dup2(pipe_write, 3)       │
    │  3. Child writes to fd 3 ──────────────▶ Goes to pipe                │
    │  4. process_channel_writes() ──────────▶ Read from pipe_read         │
    │  5. Emit tuple: ("channel_write", 3, "note", timestamp)             │
    │                                                                      │
    └────────────────────────────────┬─────────────────────────────────────┘
                                     │
                                     ▼
    ┌──────────────────────────────────────────────────────────────────────┐
    │                       TUPLE SPACE                                    │
    │                                                                      │
    │  ("channel_write", 3, "note", 1734567890)                           │
    │                                                                      │
    └────────────────────────────────┬─────────────────────────────────────┘
                                     │
                                     │ sync_channels_from_tuple_space()
                                     ▼
    ┌──────────────────────────────────────────────────────────────────────┐
    │                    CHANNEL MANAGER                                   │
    │                                                                      │
    │  ContextChannel (fd 3):                                              │
    │  ┌────────────────────────────────────────────────────────────────┐ │
    │  │ InternalBuffer                                                  │ │
    │  │   lines: [                                                      │ │
    │  │     { text: "note", timestamp: 2024-12-19T20:15:00Z }          │ │
    │  │   ]                                                             │ │
    │  └────────────────────────────────────────────────────────────────┘ │
    │                                                                      │
    └──────────────────────────────────────────────────────────────────────┘
```

### Tuple Space (Linda Coordination)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    LINDA PRIMITIVES                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   out(tuple)    ──▶  Write tuple to space (non-blocking)                   │
│   in(pattern)   ──▶  Wait for & REMOVE matching tuple (blocking)           │
│   rd(pattern)   ──▶  Wait for & READ matching tuple (blocking)             │
│   inp(pattern)  ──▶  Try to remove (non-blocking, returns Option)          │
│   rdp(pattern)  ──▶  Try to read (non-blocking, returns Option)            │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

    THREE IMPLEMENTATIONS:

    ┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐
    │   TupleSpace    │   │PersistentTuple- │   │  IpcTupleSpace  │
    │   (In-Memory)   │   │     Space       │   │  (Notifying)    │
    │                 │   │                 │   │                 │
    │ • scc::HashMap  │   │ • SQLite-backed │   │ • PersistentTS  │
    │ • Fast (~100ns) │   │ • Survives      │   │ • + IPC notify  │
    │ • Ephemeral     │   │   restart       │   │ • Cross-process │
    │                 │   │ • Write-through │   │   coordination  │
    └─────────────────┘   └─────────────────┘   └─────────────────┘
```

### Use Cases

```
    ┌─────────────────────────────────────────────────────────────────────────┐
    │  REACTIVE ENVIRONMENT                                                   │
    │                                                                         │
    │  export PATH="/new"  ───▶  tuple: ("env_change", "PATH", old, new)     │
    │  cd /other           ───▶  tuple: ("dir_change", old_dir, new_dir)     │
    │                                                                         │
    │  Agents watch for tuples and react!                                     │
    └─────────────────────────────────────────────────────────────────────────┘

    ┌─────────────────────────────────────────────────────────────────────────┐
    │  DISTRIBUTED LOCKS                                                      │
    │                                                                         │
    │  Shell A: out(("lock", "deploy", "session-a"))  ◄── Acquire            │
    │  Shell B: rdp(("lock", "deploy", _))            ◄── Check (no wait)    │
    │  Shell A: inp(("lock", "deploy", _))            ◄── Release            │
    └─────────────────────────────────────────────────────────────────────────┘

    ┌─────────────────────────────────────────────────────────────────────────┐
    │  TASK QUEUES                                                            │
    │                                                                         │
    │  Producer: out(("task", "build", description))                         │
    │  Worker:   in(("task", _, _))  ◄── Blocking wait for work              │
    └─────────────────────────────────────────────────────────────────────────┘
```

---

## Level 8: Complete Data Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    COMPLETE DATA FLOW DIAGRAM                               │
│                    From Input to Output                                      │
└─────────────────────────────────────────────────────────────────────────────┘

                                USER
                                 │
                    ┌────────────┴────────────┐
                    │     User Input          │
                    │  "ls -la | grep foo"    │
                    └────────────┬────────────┘
                                 │
           ┌─────────────────────┴─────────────────────┐
           │              VALIDATION                    │
           │                                           │
           │   Tree-Sitter ──▶ Syntax OK?             │
           │                     │                     │
           │              ┌──────┴──────┐              │
           │              │             │              │
           │           Valid      Incomplete           │
           │              │        (continue)          │
           │              ▼                            │
           │   NL Detector ──▶ Shell or Agent?        │
           │                     │                     │
           │          ┌──────────┴──────────┐          │
           │          │                     │          │
           │       SHELL                 AGENT         │
           └──────────┼─────────────────────┼──────────┘
                      │                     │
                      ▼                     ▼
           ┌──────────────────┐  ┌──────────────────┐
           │   PEST PARSER    │  │    LLM API       │
           │                  │  │                  │
           │ Source ──▶ AST   │  │ Query ──▶ Stream │
           │                  │  │                  │
           │ Statement::      │  │ Events:          │
           │   Pipeline       │  │ • TextDelta      │
           │     elements[]   │  │ • ToolCall       │
           │                  │  │ • Done           │
           └────────┬─────────┘  └────────┬─────────┘
                    │                     │
                    ▼                     ▼
           ┌──────────────────┐  ┌──────────────────┐
           │   EXECUTOR       │  │ PROTOCOL EMIT    │
           │                  │  │                  │
           │ • Expand words   │  │ • OSC 2337       │
           │ • Dispatch cmd   │  │ • stdout print   │
           │ • Fork/exec      │  │ • Widget render  │
           │ • Wait & status  │  │                  │
           └────────┬─────────┘  └────────┬─────────┘
                    │                     │
                    ▼                     ▼
           ┌──────────────────────────────────────────┐
           │              SIDE EFFECTS                │
           │                                          │
           │  • stdout/stderr output                  │
           │  • File changes                          │
           │  • Environment updates                   │
           │  • Tuple emissions (reactive)            │
           │  • Channel writes (fd 3-6)               │
           │  • History recording                     │
           │                                          │
           └──────────────────────────────────────────┘
                                 │
                                 ▼
           ┌──────────────────────────────────────────┐
           │           REPL DISPLAY                   │
           │                                          │
           │  • Command output                        │
           │  • Exit status notification              │
           │  • Tuple activity summary                │
           │    "🔄 Environment: PATH = /new/path"    │
           │  • Prompt for next input                 │
           │                                          │
           └──────────────────────────────────────────┘
                                 │
                                 ▼
                               USER
                          (Next command)
```

---

## Summary Table: Colony Shell Components

| Component | Crate | Purpose | Key Features |
|-----------|-------|---------|--------------|
| **Parser** | colony-parser | Source → AST | Pure PEG (Pest), heredoc preprocessing |
| **Expander** | colony-expand | Word expansion | POSIX order, 7 expansion types |
| **Executor** | colony-exec | Command execution | Fork/exec, pipelines, job control |
| **Agent** | colony-agent | AI integration | Multi-provider (Claude/GPT/Ollama) |
| **Tuple Space** | colony-tuple | Coordination | Linda primitives, SQLite persistence |
| **Channels** | colony-channels | Extended I/O | fd 3-6, semantic buffers |
| **Protocol** | colony-protocol | Terminal comm | OSC 2337 escape sequences |
| **Config** | colony-config | Configuration | .colonyrc, trust model |
| **Tree-sitter** | colony-tree-sitter | Editor features | Incremental CST, highlighting |
| **Memory** | colony-memory | AI memory | LanceDB vectors, SQLite metadata |
| **MCP** | colony-mcp | Tool pools | MCP server connections |
| **Focus** | colony-focus | Session tracking | Goal trees, checkpoints |

---

## Key Architectural Insights

### 1. Dual Execution Paths
- **Shell path**: Parser → Executor → Fork/Exec
- **Agent path**: NL Detector → Agent → LLM API
- Both paths share the same tuple space for reactive coordination

### 2. POSIX First, Extensions Second
- Full POSIX compatibility via `colony-sh` binary
- Modern features (agents, channels) available via `colony` binary
- Same codebase, different feature flags

### 3. Reactive Coordination
- Tuple space enables event-driven architecture
- Environment changes, directory changes, channel writes all emit tuples
- Agents and other components can subscribe to patterns

### 4. Extended I/O Model
- Traditional stdio (fd 0-2) preserved
- Semantic channels (fd 3-6) add observability
- Context, LLM history, tool calls, and traces are all capturable

### 5. Provider Abstraction
- AI agents work with multiple LLM providers
- Auto-detection: Anthropic → OpenAI → Ollama
- Ollama fallback ensures shell always works (no API key required)
