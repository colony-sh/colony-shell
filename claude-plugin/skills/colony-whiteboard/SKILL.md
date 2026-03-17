---
name: colony-whiteboard
description: Use when you need to create diagrams, architecture visualizations, brainstorm with sticky notes, plan sprints visually, draw flow diagrams, or annotate the canvas with visual elements. Uses the Colony canvas whiteboard widget.
---

# Colony Whiteboard

The Colony canvas includes a Miro-style collaborative whiteboard. It's a singleton widget — at most one per canvas. The first `draw` command creates it automatically.

## Sticky Notes

```bash
draw sticky "TODO: Fix auth" --at 100,200 --color yellow
draw sticky "API Design" --at 300,200 --color blue
draw sticky "Blocked on review" --at 100,400 --color red
draw sticky "Done!" --at 500,200 --color green
```

Colors: yellow (default), blue, green, red, purple, orange, pink

## Shapes

```bash
draw rect 50,50 400,300 --fill "rgba(0,0,255,0.1)" --label "Auth Module"
draw ellipse 500,150 200,100 --stroke red --label "Risk Area"
```

## Lines and Connectors

```bash
# Freeform lines
draw line 100,100 400,400 --arrow
draw line 100,100 400,400 --arrow --dashed

# Widget-to-widget connectors (shows relationship between canvas widgets)
draw connector <widget-id-a> <widget-id-b> --label "depends on"
```

## Text

```bash
draw text "Architecture Overview" --at 50,20 --size 24 --bold
draw text "v0.4.1" --at 50,50 --size 12
```

## Frames (Grouping)

```bash
draw frame 0,0 500,400 --label "Sprint 1" --color "rgba(0,255,0,0.05)"
draw frame 600,0 500,400 --label "Sprint 2"
```

## Manipulation

```bash
draw status                     # Whiteboard item count and size
draw move <item-id> 200,300     # Reposition an item
draw remove <item-id>           # Delete an item
draw clear                      # Clear everything
draw clear --selection <id>,<id>  # Clear specific items
```

## Export

```bash
draw export --format svg > architecture.svg
draw export --format json > whiteboard.json
draw import whiteboard.json
```

## Common Patterns

### Architecture Diagram
```bash
draw frame 0,0 800,600 --label "System Architecture"
draw rect 50,80 200,100 --fill "rgba(0,100,255,0.1)" --label "Frontend"
draw rect 300,80 200,100 --fill "rgba(0,200,0,0.1)" --label "API"
draw rect 550,80 200,100 --fill "rgba(200,0,0,0.1)" --label "Database"
draw line 250,130 300,130 --arrow
draw line 500,130 550,130 --arrow
```

### Brainstorming Grid
```bash
draw text "Brainstorm: Feature Ideas" --at 50,20 --size 20 --bold
draw sticky "Real-time collab" --at 50,80 --color yellow
draw sticky "AI code review" --at 250,80 --color yellow
draw sticky "Voice commands" --at 450,80 --color blue
draw sticky "Plugin marketplace" --at 50,230 --color green
```

### Sprint Planning
```bash
draw frame 0,0 300,500 --label "To Do" --color "rgba(255,0,0,0.03)"
draw frame 350,0 300,500 --label "In Progress" --color "rgba(255,255,0,0.03)"
draw frame 700,0 300,500 --label "Done" --color "rgba(0,255,0,0.03)"
draw sticky "Auth refactor" --at 50,60 --color yellow
draw sticky "Write tests" --at 50,180 --color yellow
draw sticky "Deploy v2" --at 400,60 --color blue
```
