---
description: View and manage Colony Shell context entries
argument-hint: [show|add <text>|clear|pin <id>|prune <tokens>]
allowed-tools: [Bash]
---

The user wants to manage Colony Shell context. Run the appropriate command:

If no arguments: `context show --tokens`
If arguments provided: `context $ARGUMENTS`

Valid subcommands:
- `show` / `show --tokens` — display context entries
- `add <text>` — add a knowledge entry
- `add --file <path>` — add file content
- `remove <id-prefix>` — remove entry
- `pin <id-prefix>` — pin entry
- `clear` — clear non-pinned entries
- `prune <max_tokens>` — prune to fit budget
