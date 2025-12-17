# Colony Reflection Engine

> Growing through observation, pattern recognition, and deliberate practice.

## Overview

Colony's reflection engine observes your daily command history and agent conversations, identifies patterns, and surfaces insights to accelerate your growth as a developer. It transforms the shell from a tool into a partner in professional development.

This isn't productivity tracking—it's **deliberate practice infrastructure** for knowledge work.

---

## Channels

```
@reflect      Accumulated observations
@patterns     Detected behavioral patterns
@growth       Suggested improvements and insights
```

---

## Observation Sources

| Category | What's Observed |
|----------|-----------------|
| **Commands** | Commands executed, timing, exit codes, duration, retries, abandonment points |
| **Agent Chats** | Questions asked, topics, clarifications needed, solutions accepted/rejected |
| **Context** | Time of day, project, git branch, focus duration, energy (inferred from patterns) |
| **Files** | Files touched, edit frequency, navigation paths, project switching |
| **Pipelines** | Common chains, bottlenecks, data transformations, repeated sequences |
| **Learning** | New tools adopted, new flags discovered, mistakes made, corrections applied |

---

## Pattern Categories

### Workflow Friction
Repeated struggles and inefficiencies:

```
@reflect: "You run 'git status' before and after 'git add' 94% of the time.
          Consider: alias gac='git add . && git status && git commit'"

@reflect: "You've asked the agent about jq 3 times with similar queries.
          Consolidation opportunity—want a cheatsheet?"
```

### Error Patterns
Recurring failures and their contexts:

```
@reflect: "Permission errors spike between 2-4pm when switching projects.
          Pattern: forgetting to re-source environment after cd.
          Suggested: Auto-source .envrc on directory change?"

@reflect: "kubectl commands fail 34% of the time with context errors.
          You work across 4 clusters but only 2 are in muscle memory.
          Consider: Context indicator in prompt + confirmation for prod?"
```

### Learning Opportunities
Tools and techniques you're ready for:

```
@reflect: "You used 'find | xargs grep' 12 times this week.
          'ripgrep' would be 10x faster and simpler: rg 'pattern'
          Want me to show equivalents for your common patterns?"

@reflect: "You're manually parsing JSON with grep/sed in 3 scripts.
          You seem to avoid jq—unfamiliarity or preference?
          I could teach jq through your actual use cases."
```

### Productivity Insights
Temporal and contextual correlations:

```
@reflect: "Your most productive sessions (by commits + agent satisfaction):
          - Tuesday/Thursday mornings, 9-11am
          - After 15+ minutes of uninterrupted focus
          - When you start with 'make test' (TDD pattern?)
          
          Your least productive:
          - High context-switching days (4+ project directories)
          - When you skip your usual 'git pull' ritual"
```

### Agent Interaction Patterns
How you collaborate with AI:

```
@reflect: "You get best results when:
          - You provide concrete examples upfront
          - You specify output format explicitly
          - You ask for step-by-step thinking
          
          You abandon conversations when:
          - Agent gives walls of text (you prefer concise)
          - You have to re-explain context (memory gap?)
          
          Suggested preamble based on your style:
          'Be concise. Think step-by-step. Ask if unclear.'"
```

### Growth Trajectory
Progress over time:

```
@reflect: "Monthly growth report:

          Tools mastered:        +fd, +ripgrep, +jq basics
          Recurring struggles:   awk (still copying from Stack Overflow)
          Automation created:    3 new aliases, 1 function
          Agent reliance:        ↓ for git, ↑ for k8s (learning curve)
          Error rate:            ↓ 15% overall
          
          You're building kubectl knowledge rapidly.
          Want to consolidate with a focused session?"
```

---

## Interface

### Manual Triggers

```colony
reflect now                      # Immediate analysis
reflect today                    # Today's patterns
reflect week                     # Weekly summary
reflect month                    # Monthly growth report
```

### Specific Inquiries

```colony
reflect "why do I keep messing up docker commands?"
reflect "what have I learned this month?"
reflect "when am I most productive?"
reflect "show me my growth with kubernetes"
```

### Interactive Session

```colony
reflect session
> What patterns do you notice in my error recovery?
> Am I getting better at shell scripting?
> What should I focus on learning next?
> Show me my most repeated command sequences
```

### Configuration

```colony
reflect config schedule "daily 6pm"     # When to analyze
reflect config depth week               # Analysis window
reflect config sensitivity medium       # How often to surface insights
reflect config style encouraging        # Tone: encouraging|neutral|challenging
reflect config focus learning           # Priority: learning|productivity|both
```

---

## Output Formats

### Daily Digest (Automatic)

```
┌─────────────────────────────────────────────────────────────────┐
│  📊 Daily Reflection — Tuesday, Dec 17                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Session: 6.2 hours │ 847 commands │ 12 agent chats            │
│                                                                 │
│  ✨ Wins                                                        │
│  • First successful use of 'fd' instead of 'find'              │
│  • Completed k8s deployment without agent help                 │
│  • New alias 'glog' saved ~2 min                               │
│                                                                 │
│  🔄 Patterns                                                    │
│  • Repeated: ls → cd → ls (47 times) — try 'z' or 'zoxide'    │
│  • Asked agent about jq 3x with similar queries                │
│                                                                 │
│  💡 Observation                                                 │
│  Morning (9-11am): 0 errors                                    │
│  Afternoon (3-5pm): 12 errors, 8 retries                       │
│  Energy management or task complexity?                         │
│                                                                 │
│  🎯 Tomorrow                                                    │
│  Practice: jq filters (you're close)                           │
│  Review: Yesterday's docker-compose trouble                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Weekly Growth Report

```
┌─────────────────────────────────────────────────────────────────┐
│  📈 Weekly Growth — Dec 11-17                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  SKILL EVOLUTION                                                │
│  find + grep    ████████░░░░  → ripgrep                        │
│  docker         ████████████  (stable)                         │
│  kubectl        ░░░░████████  ← learning                       │
│  jq             ░░░░░░██████  ← breakthrough                   │
│                                                                 │
│  AGENT COLLABORATION                                            │
│  Questions: 67 (↓ from 84)                                     │
│  Self-resolved: 71% (↑ from 58%)                               │
│  Topics: k8s 34%, rust 22%, shell 18%                          │
│                                                                 │
│  FRICTION REDUCED                                               │
│  • 4 aliases saved ~45 min total                               │
│  • git errors ↓ 60%                                            │
│  • docker-compose issues ↓ 40%                                 │
│                                                                 │
│  STILL STRUGGLING                                               │
│  • awk: 100% copied from search (8 instances)                  │
│  • tmux: abandoned 3x, used multiple terminals                 │
│  • regex in sed: 4/5 needed agent help                         │
│                                                                 │
│  INSIGHT                                                        │
│  You learn fastest through immediate need.                     │
│  Scheduled study doesn't stick as well.                        │
│  Consider: just-in-time over planned learning?                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Privacy & Control

All data stays local by default. You control everything.

### Storage

```colony
reflect config storage local         # Never leaves machine (default)
reflect config storage encrypted     # At-rest encryption
```

### Exclusions

```colony
reflect exclude path ~/personal      # Never observe directory
reflect exclude path ~/work/secret   # Multiple exclusions supported
reflect exclude command "pass*"      # Never log password manager
reflect exclude command "*SECRET*"   # Pattern matching
reflect exclude chat medical         # Topic-based exclusion
reflect exclude chat personal        # Skip personal conversations
```

### Analysis Control

```colony
reflect config agent local           # Use local LLM for analysis
reflect config agent cloud           # Use cloud (explicit consent)
reflect config agent none            # Rule-based only, no LLM
```

### Data Lifecycle

```colony
reflect retention 90d                # Auto-delete after 90 days
reflect retention unlimited          # Keep forever
reflect export                       # Export your data (JSON)
reflect export --format=markdown     # Human-readable export
reflect purge                        # Delete everything
reflect purge --before=2024-01-01    # Selective deletion
```

### Transparency

```colony
reflect explain last                 # Why was this insight shown?
reflect show-data today              # Raw observations from today
reflect show-data --category=errors  # Filter by category
```

---

## Integration

### Agent Preamble (Automatic)

Reflection insights inform agent interactions:

```colony
agent "help with docker"
# Agent automatically receives:
# "User learns best with concise examples.
#  Previous docker struggles: volume mounting, networking.
#  Preferred format: command first, explanation after."
```

### Smart Completions

```colony
$ docker <tab>
# Prioritizes subcommands you're learning over mastered ones
# Surfaces commands related to recent errors
```

### Context Sharing

```colony
# Reflection populates tuple space for multi-agent awareness
out @context {
    user_learning: ["kubernetes", "jq"],
    user_mastered: ["git", "docker-basics"],
    user_style: "concise, examples-first",
    user_productive_hours: ["9am-11am", "2pm-4pm"]
}
```

### Capability Suggestions

```colony
$ agent --caps=??? "deploy to production"
@reflect: "You've been learning k8s. Want read-only caps first
          so you can observe before mutating?"
```

---

## Implementation Notes

### Core Types

```rust
pub struct Observation {
    timestamp: DateTime<Utc>,
    category: ObservationCategory,
    context: SessionContext,
    data: ObservationData,
}

pub enum ObservationCategory {
    Command { cmd: String, exit_code: i32, duration: Duration },
    AgentChat { turns: u32, topic: String, resolution: Resolution },
    FileActivity { path: PathBuf, action: FileAction },
    Error { kind: ErrorKind, recovery: Option<RecoveryAction> },
    Pattern { name: String, instance_count: u32 },
}

pub struct SessionContext {
    time_of_day: TimeOfDay,
    day_of_week: Weekday,
    project: Option<String>,
    git_branch: Option<String>,
    focus_duration: Duration,
    energy_estimate: EnergyLevel,  // Derived from error rate, typing patterns
}

pub struct Pattern {
    name: String,
    frequency: u32,
    confidence: f64,
    category: PatternCategory,
    suggestion: Option<String>,
}

pub enum PatternCategory {
    Friction,
    ErrorProne,
    Learning,
    Productivity,
    Communication,
}
```

### Analysis Pipeline

```
Observations → Window (day/week/month)
            → Pattern Detection (rule-based + ML)
            → Insight Generation (LLM or templates)
            → Filtering (sensitivity, relevance)
            → Presentation (digest, interactive, inline)
```

---

## Philosophy

Anders Ericsson's research on expertise shows growth requires:

1. **Immediate feedback** — Colony's errors + agent responses
2. **Focus on weaknesses** — Reflection surfaces these automatically
3. **Intentional practice** — Suggested focus areas
4. **Mental models** — Agent conversations externalize thinking

Most developers plateau because they lack this loop. They use the same commands for decades. Colony's reflection engine creates the feedback mechanism that accelerates expertise.

The goal isn't to track you—it's to **help you see yourself** the way a great mentor would: noticing patterns you're blind to, celebrating progress you've forgotten, and pointing toward growth you're ready for.