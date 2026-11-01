---
name: colony-whiteboard
description: Use when you need to draw on the Colony canvas — architecture or flow diagrams, sticky-note brainstorms, sprint boards, or boxes and connectors that show how canvas items relate.
---

# Colony Whiteboard

`draw` writes shapes, cards, lines, connectors and frames onto the Bellman room's canvas (`$BELLMAN_ROOM`). `draw help` lists every flag; the `colony-canvas-awareness` skill covers keys and sharing.

## What draws what

| Command | Item | Notes |
|---|---|---|
| `draw sticky <text>`, `draw text <text>` | text card | No colour. Without `--position`, Bellman places it in the next free slot |
| `draw rect [label]`, `draw ellipse [label]` | shape | Default 200x150 at 100,100 |
| `draw line --from X,Y --to X,Y` | line shape | No arrowheads |
| `draw connector <from-key> <to-key> [--label L]` | connector | Joins two items by key; it has no position of its own |
| `draw frame [label]` | frame | Default 600x400; groups what sits inside it |

Colours (`--color`, shapes only): `slate` (default), `blue`, `green`, `amber`, `red`, `violet`, or a hex such as `#3b82f6`.

Give items you will connect a `--key`, so the connector can name them.

## Architecture diagram

```bash
draw frame "System Architecture" --key arch -p 0,0 -s 820x300
draw rect Frontend --key fe -p 40,80 -s 200x120 -c blue
draw rect API --key api -p 310,80 -s 200x120 -c green
draw rect Database --key db -p 580,80 -s 200x120 -c amber
draw connector fe api --label "HTTPS"
draw connector api db --label "SQL"
```

## Sprint board

```bash
draw frame "To Do" --key todo -p 0,0 -s 300x500
draw frame "In Progress" --key doing -p 340,0 -s 300x500
draw frame "Done" --key done -p 680,0 -s 300x500
draw sticky "Auth refactor" --key auth -p 40,60
draw sticky "Write tests" --key tests -p 40,280
```

Move a card between columns with `canvas widget move auth 380,60`.
