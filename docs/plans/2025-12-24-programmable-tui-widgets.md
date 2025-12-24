# Programmable TUI Widgets for Colony Shell

## Overview

Enable per-directory TUI widgets that can be defined in `.colonyrc` or dedicated widget definition files. These widgets render in a sidepanel and provide contextual information/interactions based on the current directory context.

## Use Cases

### Git Widget
- Shows in any git repository automatically
- Displays: current branch, ahead/behind status, staged/unstaged changes, stash count
- Interactive: stage files, commit, push, pull via keybindings
- Updates reactively when filesystem changes

### Project Status Widget
- Node.js: shows package.json scripts, dependencies status, audit warnings
- Rust: shows Cargo.toml targets, build status, clippy warnings
- Python: shows venv status, requirements, test status

### Custom Widgets
- User-defined widgets via `.colony/widgets/` directory
- Each widget is a shell script or function that outputs widget data
- Support for JSON/structured output for rich rendering

## Architecture

### Widget Definition Format
```toml
# .colony/widgets/git-status.toml
[widget]
name = "git-status"
title = "Git Status"
trigger = { directory_contains = ".git" }
position = "right"  # left, right, top, bottom
width = 40
refresh = "on_change"  # on_change, interval:5s, manual

[widget.content]
type = "script"
command = "colony-widget-git-status"
format = "structured"  # structured, ansi, markdown
```

### Widget Protocol
Widgets communicate via structured output:
```json
{
  "sections": [
    {
      "title": "Branch",
      "content": "main",
      "style": "bold"
    },
    {
      "title": "Changes",
      "items": [
        {"text": "M src/main.rs", "style": "yellow"},
        {"text": "A README.md", "style": "green"}
      ]
    }
  ],
  "actions": [
    {"key": "s", "label": "Stage all", "command": "git add -A"},
    {"key": "c", "label": "Commit", "command": "widget:git-commit-dialog"}
  ]
}
```

### Integration Points

1. **colony-shell**: 
   - Widget registry and lifecycle management
   - Widget script execution
   - OSC 2337 emission for widget updates

2. **colony-terminal** (future):
   - Widget rendering in sidepanels
   - Keyboard shortcut routing to widgets
   - Layout management

3. **Tuple Space**:
   - Widget state stored in tuples
   - Reactive updates on directory/file changes
   - Cross-widget communication

## Implementation Plan

### Phase 1: Widget Infrastructure
- [ ] Widget definition parser (.toml format)
- [ ] Widget registry in ExpansionContext
- [ ] Widget discovery on directory change
- [ ] `widget` builtin for manual widget control

### Phase 2: Built-in Widgets
- [ ] git-status widget (branch, changes, actions)
- [ ] directory-info widget (file count, sizes)
- [ ] env-vars widget (show current environment)

### Phase 3: Custom Widgets
- [ ] Script-based widget execution
- [ ] Structured output parsing
- [ ] Widget template system

### Phase 4: Terminal Integration
- [ ] OSC 2337 Widget= protocol extension
- [ ] Sidepanel layout negotiation
- [ ] Interactive widget actions

## Open Questions

1. Should widgets be able to modify shell state (variables, directory)?
2. How to handle widget errors/crashes gracefully?
3. Widget permission model - which widgets can execute commands?
4. Widget inheritance - project widgets vs user widgets vs system widgets?
