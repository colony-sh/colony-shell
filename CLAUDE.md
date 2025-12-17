
## Session Start Protocol

At the beginning of EVERY coding session, execute these steps IN ORDER:

### 1. Check Git Status
```bash
git status
git log --oneline -10
```

Review what's changed since last session. Note any uncommitted work.

### 2. Read System Files
```bash
cat .claude/system/rules.txt
cat .claude/system/features.json
tail -50 .claude/system/history.txt
```

Understand: What are the constraints? What features exist and their status? What was done recently?

### 3. Identify Current Focus

From features.json, find:
- Features with status "in_progress" — these are active work
- Features with status "blocked" — check if blockers are resolved
- Next "new" feature by priority (lowest number = highest priority) whose dependencies are "complete"

### 4. Announce Plan

Before writing any code, state:
- What you found in git history and system files
- What feature you're working on and why
- What specific task you'll do this session
- Any blockers or questions

---

## Updating System Files

### Updating features.json

When starting work on a feature:
```json
{
  "id": "F001",
  "status": "in_progress",
  "notes": ["Started YYYY-MM-DD: Initial lexer structure"]
}
```

When completing a feature:
```json
{
  "id": "F001", 
  "status": "complete",
  "notes": ["Started YYYY-MM-DD: Initial lexer structure", "Completed YYYY-MM-DD: All tests passing"]
}
```

When blocked:
```json
{
  "id": "F001",
  "status": "blocked",
  "notes": ["BLOCKED: Need decision on how to handle heredoc edge case X"]
}
```

### Updating history.txt

Log significant actions as you work:
```
[2025-01-15 14:30] START: Beginning work on F001 (Lexer)
[2025-01-15 14:45] PROGRESS: Token enum defined, basic word tokenization working
[2025-01-15 15:30] DECISION: Using mode stack approach for quote handling
[2025-01-15 16:00] PROGRESS: Single and double quote modes implemented
[2025-01-15 17:00] BUG: Heredoc delimiter not detected when followed by comment
[2025-01-15 17:30] FIX: Added lookahead for heredoc detection
[2025-01-15 18:00] COMPLETE: F001 lexer complete, 47 tests passing
```

### Updating rules.txt

Add rules when you discover important constraints:
```
27. Heredoc content must be read AFTER the command line is fully parsed — not inline
28. SIGCHLD handler must be async-signal-safe — no allocations
```

---

## Session End Protocol

Before ending a session:

1. **Commit work** with descriptive message referencing feature ID
2. **Update features.json** with current status and notes
3. **Add history.txt entries** for all significant work done
4. **Summarize** what was accomplished and what's next

---

## Example Session Flow
```
Agent: Starting session. Let me check git and system files...

[Runs git status, git log]
[Reads rules.txt, features.json, history.txt]

Agent: Git shows last commit was "F001: Complete lexer with mode stack" from yesterday.
Features.json shows F001 is complete, F002 (parser) is "new" with dependency on F001 satisfied.
History shows lexer was completed with 47 tests.

Plan for this session: Begin F002 (Recursive descent parser)
- First task: Define AST node types for simple commands and pipelines
- Will update features.json to "in_progress" now

[Updates features.json]
[Logs: START: Beginning work on F002 (Parser)]

... coding work ...

[Logs: PROGRESS: AST types defined, simple command parsing working]
[Logs: DECISION: Using Box<Command> for pipeline elements to allow recursion]

Agent: Ending session. Committing work and updating tracking.

[git commit -m "F002: AST types and simple command parsing"]
[Updates history.txt with session summary]
[Updates features.json notes]

Summary: Defined AST node types, implemented simple command and pipeline parsing.
Next session: Implement compound commands (if/while/for) parsing.
```

---

## Important Reminders

- ALWAYS read system files at session start — no exceptions
- ALWAYS update files as you work — not just at the end
- Feature IDs (F001, F002...) are permanent — never renumber
- History is append-only — never delete entries
- Rules accumulate — add discoveries, don't remove old rules
- When uncertain, add a DECISION entry explaining your reasoning
- When stuck, set status to "blocked" with clear description

Your effectiveness depends on maintaining this system. Future you (and other agents) rely on accurate tracking to understand context quickly.
- DO NOT STOP A TASK DUE TO COMPLEXITY! ALWAYS SEE IT THROUGH UNLESS THE USER STOPS YOU!