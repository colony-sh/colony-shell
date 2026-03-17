---
name: colony-canvas-awareness
description: Use when COLONY_CANVAS_ID env var is present, when the user mentions the Colony canvas, when you need to interact with canvas widgets, check who else is on the canvas, announce your working status, create or modify widgets, or coordinate with other participants.
---

# Colony Canvas Awareness

You are running on a **Colony Nerve canvas** — a spatial workspace where humans and AI agents collaborate in real time. Your terminal is one widget on this canvas alongside others (browsers, task boards, timers, Zed editor viewports, etc.).

## Environment

```bash
echo $COLONY_CANVAS_ID          # Canvas UUID
echo $COLONY_CANVAS_NAME        # Human-readable name
echo $COLONY_API                # Backend URL (https://api.colony.sh)
echo $COLONY_SESSION_TOKEN      # Auth token (user's JWT)
echo $COLONY_WIDGET_ID          # Your terminal widget's ID
echo $COLONY_USER_ID            # Current user's ID
```

## Canvas State

```bash
canvas status                   # Canvas name, participants, widget count
canvas participants             # List humans + Claude instances with status
canvas widgets                  # List all widgets with type, position, size
```

## Announcing Your Status

**Always announce what you're working on** so other participants (humans and Claudes) can see:

```bash
canvas announce "refactoring auth module"
canvas announce --files src/auth.rs src/auth_test.rs
canvas announce --clear                  # When done
```

Other Claudes see your status via `canvas participants`. Check before working on shared files.

## Widget Management

```bash
# Create widgets
canvas widget create markdown --title "Notes" --position 100,500 --size 400x300
canvas widget create terminal --command "cargo test" --position 600,500

# Modify widgets
canvas widget move <id> 200,300
canvas widget resize <id> 600x400
canvas widget remove <id>

# Read/write widget config
canvas widget config <id>               # Print JSON config
canvas widget config <id> '{"title": "Updated"}'  # Patch config
```

## Reading Widget Content

```bash
canvas tasks                    # Read task board items
canvas markdown <id>            # Read markdown widget content
```

## Drawing (Whiteboard)

The canvas has a Miro-style whiteboard widget. See the `colony-whiteboard` skill for full drawing commands. Quick reference:

```bash
draw sticky "TODO" --at 100,200 --color yellow
draw rect 50,50 400,300 --label "Module"
draw connector <widget-a> <widget-b> --label "depends on"
```

## Multi-Agent Best Practices

1. **Check participants first:** `canvas participants` before starting work
2. **Announce early:** `canvas announce "working on X"` before touching files
3. **Check for conflicts:** look at other agents' `--files` in participant list
4. **Clear when done:** `canvas announce --clear` when task is complete
5. **Use the whiteboard** for architecture discussions visible to all participants

## REST API (Advanced)

All `canvas` builtins are thin wrappers around REST calls. For advanced use:

```bash
curl -s -H "Authorization: Bearer $COLONY_SESSION_TOKEN" \
  "$COLONY_API/api/canvases/$COLONY_CANVAS_ID/participants"
```
