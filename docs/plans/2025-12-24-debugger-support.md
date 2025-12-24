# Shell Script Debugger Support

## Overview

Implement comprehensive debugging capabilities for shell scripts, including breakpoints, stepping, variable inspection, and call stack visualization.

## Features

### Core Debugging Commands

```bash
# Start script in debug mode
colony --debug script.sh

# Or enable debugging in running shell
set -o debug
debug enable

# Breakpoint commands
debug break 10           # Break at line 10
debug break func_name    # Break when entering function
debug break --condition '$x > 5' 15  # Conditional breakpoint
debug list               # List all breakpoints
debug delete 1           # Delete breakpoint #1

# Execution control
debug step              # Execute one statement (step into)
debug next              # Execute one statement (step over)
debug continue          # Continue until next breakpoint
debug finish            # Run until current function returns
debug run               # Start/restart execution

# Inspection
debug print $var        # Print variable value
debug print -a arr      # Print array contents
debug print -f func     # Print function definition
debug locals            # Show all local variables
debug globals           # Show all global variables
debug env               # Show environment variables
debug stack             # Show call stack
debug trace             # Toggle execution trace

# Watch expressions
debug watch '$x'        # Break when $x changes
debug watch --expr '$x + $y > 10'  # Break when expression becomes true
```

### Debug UI (via OSC 2337)

The debugger emits structured debug state to the terminal:

```json
{
  "type": "debug_state",
  "location": {
    "file": "script.sh",
    "line": 42,
    "column": 1,
    "function": "process_file"
  },
  "call_stack": [
    {"function": "main", "file": "script.sh", "line": 10},
    {"function": "process_all", "file": "script.sh", "line": 25},
    {"function": "process_file", "file": "script.sh", "line": 42}
  ],
  "locals": {
    "filename": "/path/to/file",
    "count": "42"
  },
  "breakpoints": [
    {"id": 1, "line": 42, "enabled": true, "hits": 3}
  ],
  "status": "paused"
}
```

## Architecture

### Debug State Machine
```
IDLE -> RUNNING -> PAUSED -> RUNNING -> ...
                      |
                      v
                  STEPPING -> PAUSED
```

### Components

1. **DebugContext**: Tracks debug state, breakpoints, watch expressions
2. **DebugController**: Handles debug commands, manages execution
3. **LineTracker**: Tracks current execution location (file, line, function)
4. **BreakpointManager**: Evaluates breakpoint conditions
5. **WatchManager**: Evaluates watch expressions on variable changes

### Integration Points

- **Parser**: Emit source locations (Span) in AST nodes
- **Executor**: Check breakpoints before each statement
- **ExpansionContext**: Notify debugger on variable changes
- **REPL**: Debug command parsing and execution

## Tutorial Mode Extension

Tutorial mode leverages the debugger to provide educational walkthroughs:

```bash
# Start tutorial mode for a script
colony --tutorial script.sh

# Or interactively
tutorial script.sh
```

### Tutorial Features

1. **Parse Visualization**
   - Show input being tokenized
   - Highlight grammar rules being matched
   - Display parse tree construction step-by-step

2. **AST Visualization**
   - Show AST nodes being created
   - Explain each node type
   - Display tree structure graphically

3. **Expansion Walkthrough**
   - Show each expansion phase (tilde, parameter, command sub, etc.)
   - Highlight what's being expanded
   - Show intermediate results

4. **Execution Trace**
   - Step through execution automatically with explanations
   - Show control flow decisions
   - Display variable state changes

### Tutorial Output Format

```json
{
  "type": "tutorial_step",
  "phase": "parse",
  "step": 3,
  "description": "Matching command substitution $(pwd)",
  "highlight": {
    "input": "echo $(pwd)",
    "start": 5,
    "end": 11
  },
  "grammar_rule": "DollarCommandSubRaw",
  "explanation": "The $(...) syntax creates a command substitution. The command inside will be executed and its output captured.",
  "ast_preview": {
    "type": "WordPart::CommandSubRaw",
    "content": "pwd"
  }
}
```

## Implementation Plan

### Phase 1: Debug Infrastructure
- [ ] DebugContext struct with breakpoints and state
- [ ] Line tracking during execution (LINENO integration)
- [ ] Basic breakpoint support (line-based)
- [ ] `debug` builtin with basic commands

### Phase 2: Execution Control
- [ ] Step, next, continue, finish commands
- [ ] Call stack tracking
- [ ] Function entry/exit breakpoints
- [ ] Conditional breakpoints

### Phase 3: Inspection
- [ ] Variable inspection (locals, globals, env)
- [ ] Watch expressions
- [ ] Expression evaluation in debug context
- [ ] Stack frame navigation

### Phase 4: Tutorial Mode
- [ ] Parse visualization hooks
- [ ] AST pretty-printing with explanations
- [ ] Expansion phase tracing
- [ ] Interactive step-through UI

### Phase 5: Terminal Integration
- [ ] OSC 2337 Debug= protocol
- [ ] Source code display with breakpoint markers
- [ ] Variable watch panel
- [ ] Call stack panel

## DAP (Debug Adapter Protocol) Support

Future consideration: Implement DAP for IDE integration (VS Code, etc.)

```
IDE <-> DAP <-> colony --debug-server
```

This would allow debugging Colony scripts directly from VS Code with full IDE features.
