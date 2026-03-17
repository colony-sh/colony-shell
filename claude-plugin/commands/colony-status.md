---
description: Show Colony Shell and canvas integration status
allowed-tools: [Bash]
---

Run the following commands to show the current Colony integration status:

```bash
echo "=== Colony Shell ==="
if [ -n "$COLONY_SHELL" ]; then
  agent status 2>/dev/null || echo "Agent: not initialized"
  echo ""
  echo "=== Context ==="
  context show --tokens 2>/dev/null || echo "No context entries"
else
  echo "Not running in Colony Shell"
fi

if [ -n "$COLONY_CANVAS_ID" ]; then
  echo ""
  echo "=== Canvas ==="
  canvas status 2>/dev/null || echo "Canvas builtins not available"
  echo ""
  echo "=== Participants ==="
  canvas participants 2>/dev/null || echo "Cannot fetch participants"
fi
```

Report the results to the user concisely.
