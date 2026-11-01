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

if [ -n "$BELLMAN_ROOM" ]; then
  echo ""
  echo "=== Canvas ==="
  canvas status 2>&1 || true
  echo ""
  echo "=== Participants ==="
  canvas participants 2>&1 || true
fi
```

Report the results to the user concisely.
