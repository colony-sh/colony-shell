# Part 17: Reflection Engine

## 17.1 Overview

The reflection engine observes command history and agent interactions, identifies patterns, and surfaces insights to accelerate developer growth. It transforms the shell from a tool into a partner in professional development.

### 17.1.1 Design Goals

1. **Observable** - Track commands, errors, agent interactions, context usage
2. **Pattern-aware** - Detect recurring behaviors, friction points, learning curves
3. **Actionable** - Surface insights as concrete suggestions
4. **Privacy-first** - All data local by default, explicit opt-in for cloud
5. **Non-intrusive** - Insights appear when relevant, not constantly

### 17.1.2 Channels

Reflection uses three virtual channels:

| Channel | Purpose |
|---------|---------|
| `@reflect` | Accumulated observations |
| `@patterns` | Detected behavioral patterns |
| `@growth` | Suggested improvements and insights |

These are not file descriptors but named buffers accessible via channel syntax.

---

## 17.2 Observation System

### 17.2.1 Observation Types

```rust
pub struct Observation {
    pub id: ObservationId,
    pub timestamp: DateTime<Utc>,
    pub category: ObservationCategory,
    pub context: SessionContext,
    pub data: ObservationData,
}

pub enum ObservationCategory {
    Command {
        cmd: String,
        args: Vec<String>,
        exit_code: i32,
        duration: Duration,
        retries: u32,
    },
    AgentChat {
        turns: u32,
        topic: String,
        resolution: Resolution,
        tokens_used: usize,
    },
    FileActivity {
        path: PathBuf,
        action: FileAction,
    },
    Error {
        kind: ErrorKind,
        recovery: Option<RecoveryAction>,
    },
    ContextUsage {
        sources_added: Vec<SourceId>,
        sources_referenced: Vec<SourceId>,
        token_count: usize,
    },
}

pub enum Resolution {
    Solved,
    Abandoned,
    Escalated,
    Deferred,
}
```

### 17.2.2 Session Context

```rust
pub struct SessionContext {
    pub session_id: SessionId,
    pub time_of_day: TimeOfDay,
    pub day_of_week: Weekday,
    pub project: Option<String>,
    pub git_branch: Option<String>,
    pub focus_duration: Duration,
    pub energy_estimate: EnergyLevel,
}

pub enum EnergyLevel {
    High,    // Low error rate, fast typing
    Normal,
    Low,     // High error rate, many retries
}
```

### 17.2.3 Observation Storage

```
~/.colony/
└── reflection/
    ├── observations/
    │   ├── 2024-12-17.obs    # Daily observation files
    │   └── ...
    ├── patterns/
    │   └── detected.json     # Detected patterns
    ├── insights/
    │   └── pending.json      # Unshown insights
    └── config.toml           # Reflection settings
```

---

## 17.3 Pattern Detection

### 17.3.1 Pattern Types

```rust
pub struct Pattern {
    pub id: PatternId,
    pub name: String,
    pub category: PatternCategory,
    pub frequency: u32,
    pub confidence: f64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub suggestion: Option<String>,
}

pub enum PatternCategory {
    WorkflowFriction,   // Repeated inefficiencies
    ErrorProne,         // Recurring failures
    LearningCurve,      // Tools being learned
    Productivity,       // Time/energy correlations
    Communication,      // Agent interaction patterns
}
```

### 17.3.2 Detection Rules

**Workflow Friction:**
```rust
// Detect: same command sequence repeated N times
// Example: ls → cd → ls (navigation pattern)
rule RepetitiveSequence {
    trigger: sequence_count(cmd_pattern) > 10,
    suggestion: "Consider alias or function for this pattern",
}

// Detect: command retried after failure
rule RetryPattern {
    trigger: same_cmd_within(30.seconds) && prev_exit_code != 0,
    suggestion: "Frequent retries suggest confusion about {cmd}",
}
```

**Error Patterns:**
```rust
// Detect: errors clustered by time of day
rule TemporalErrors {
    trigger: error_rate(time_window) > baseline * 2,
    suggestion: "Error rate spikes {time_range}. Energy management?",
}

// Detect: errors by project/context
rule ContextualErrors {
    trigger: error_rate(project) > global_avg * 1.5,
    suggestion: "Higher error rate in {project}. Unfamiliar codebase?",
}
```

**Learning Opportunities:**
```rust
// Detect: tool underutilization
rule ToolDiscovery {
    trigger: uses_slow_alternative(tool) && fast_alternative_exists,
    suggestion: "You use {slow} for this. Try {fast} instead.",
}

// Detect: agent dependency for topic
rule AgentCrutch {
    trigger: agent_queries(topic) > 3 && self_resolved_rate < 0.5,
    suggestion: "You often ask about {topic}. Want a focused learning session?",
}
```

### 17.3.3 Pattern Confidence

```rust
impl Pattern {
    pub fn confidence(&self) -> f64 {
        // Higher frequency = higher confidence
        let freq_factor = (self.frequency as f64).log10() / 3.0;

        // Recent patterns weighted more
        let recency = self.last_seen.signed_duration_since(Utc::now());
        let recency_factor = (-recency.num_days() as f64 / 30.0).exp();

        (freq_factor * recency_factor).min(1.0)
    }
}
```

---

## 17.4 Insight Generation

### 17.4.1 Insight Types

```rust
pub struct Insight {
    pub id: InsightId,
    pub category: InsightCategory,
    pub title: String,
    pub body: String,
    pub patterns: Vec<PatternId>,
    pub priority: Priority,
    pub shown: bool,
    pub dismissed: bool,
}

pub enum InsightCategory {
    DailyDigest,
    WeeklyGrowth,
    ImmediateSuggestion,
    LearningOpportunity,
    ProductivityTip,
}
```

### 17.4.2 Generation Pipeline

```
Observations → Window (day/week/month)
            → Pattern Detection (rule-based)
            → Insight Generation (LLM or templates)
            → Filtering (sensitivity, relevance)
            → Presentation (digest, inline, interactive)
```

### 17.4.3 LLM Integration

```rust
impl ReflectionEngine {
    pub async fn generate_insight(&self, patterns: &[Pattern]) -> Result<Insight> {
        let prompt = self.build_insight_prompt(patterns);

        // Use local LLM by default
        let response = match self.config.analysis_mode {
            AnalysisMode::Local => self.local_llm.generate(&prompt).await?,
            AnalysisMode::Cloud => self.cloud_llm.generate(&prompt).await?,
            AnalysisMode::RuleBased => self.template_insight(patterns)?,
        };

        self.parse_insight(response)
    }
}
```

---

## 17.5 Reflect Builtin

### 17.5.1 Commands

```
reflect [now|today|week|month]    Show reflection report
reflect "<question>"              Ask about your patterns
reflect session                   Interactive reflection session
reflect config <key> <value>      Configure reflection
```

### 17.5.2 Configuration

```
reflect config schedule "daily 6pm"      When to analyze
reflect config depth week                Analysis window
reflect config sensitivity medium        How often to surface insights
reflect config style encouraging         Tone: encouraging|neutral|challenging
reflect config focus learning            Priority: learning|productivity|both
reflect config agent local               Use local LLM (default)
reflect config agent cloud               Use cloud LLM (opt-in)
reflect config agent none                Rule-based only
```

### 17.5.3 Privacy Controls

```
reflect exclude path ~/personal          Never observe directory
reflect exclude command "pass*"          Never log command pattern
reflect exclude chat <topic>             Skip conversations on topic
reflect retention 90d                    Auto-delete after 90 days
reflect export [--format=json|md]        Export your data
reflect purge [--before=date]            Delete data
```

---

## 17.6 Output Formats

### 17.6.1 Daily Digest

```
┌─────────────────────────────────────────────────────────────────┐
│  📊 Daily Reflection — Tuesday, Dec 17                          │
├─────────────────────────────────────────────────────────────────┤
│  Session: 6.2 hours │ 847 commands │ 12 agent chats            │
│                                                                 │
│  ✨ Wins                                                        │
│  • First successful use of 'fd' instead of 'find'              │
│  • Completed deployment without agent help                      │
│                                                                 │
│  🔄 Patterns                                                    │
│  • Repeated: ls → cd → ls (47 times) — try 'z' or 'zoxide'    │
│                                                                 │
│  💡 Observation                                                 │
│  Morning (9-11am): 0 errors                                    │
│  Afternoon (3-5pm): 12 errors                                   │
└─────────────────────────────────────────────────────────────────┘
```

### 17.6.2 Weekly Growth Report

```
┌─────────────────────────────────────────────────────────────────┐
│  📈 Weekly Growth — Dec 11-17                                   │
├─────────────────────────────────────────────────────────────────┤
│  SKILL EVOLUTION                                                │
│  kubectl        ░░░░████████  ← learning                       │
│  jq             ░░░░░░██████  ← breakthrough                   │
│                                                                 │
│  AGENT COLLABORATION                                            │
│  Questions: 67 (↓ from 84)                                     │
│  Self-resolved: 71% (↑ from 58%)                               │
│                                                                 │
│  STILL STRUGGLING                                               │
│  • awk: 100% copied from search                                │
└─────────────────────────────────────────────────────────────────┘
```

---

## 17.7 Integration

### 17.7.1 Agent Preamble

Reflection informs agent interactions:

```rust
impl Agent {
    fn build_preamble(&self, reflection: &ReflectionEngine) -> String {
        let prefs = reflection.user_preferences();
        format!(
            "User learns best with {}. Previous struggles: {}. Preferred format: {}.",
            prefs.learning_style,
            prefs.recent_struggles.join(", "),
            prefs.output_format
        )
    }
}
```

### 17.7.2 Context Integration

Reflection populates tuple space:

```colony
out @context {
    user_learning: ["kubernetes", "jq"],
    user_mastered: ["git", "docker-basics"],
    user_style: "concise, examples-first",
    user_productive_hours: ["9am-11am", "2pm-4pm"]
}
```

### 17.7.3 Completion Integration

```rust
impl CompletionEngine {
    fn prioritize(&self, candidates: &mut [Completion], reflection: &ReflectionEngine) {
        // Boost commands user is learning
        for c in candidates.iter_mut() {
            if reflection.is_learning(&c.command) {
                c.score *= 1.5;
            }
        }
    }
}
```

---

## 17.8 Implementation Notes

### 17.8.1 Crate Structure

```
colony-reflection/
├── src/
│   ├── lib.rs
│   ├── observer.rs      # Observation collection
│   ├── storage.rs       # Observation persistence
│   ├── patterns.rs      # Pattern detection
│   ├── insights.rs      # Insight generation
│   ├── builtin.rs       # reflect command
│   └── integration.rs   # Agent/context hooks
```

### 17.8.2 Performance

- Observations buffered in memory, flushed periodically
- Pattern detection runs async, not blocking commands
- Insights generated on-demand or scheduled (not real-time)

---

## 17.9 Tier Availability

| Feature | Free | Pro | Team | Enterprise |
|---------|------|-----|------|------------|
| Local observation | Yes | Yes | Yes | Yes |
| Rule-based patterns | Yes | Yes | Yes | Yes |
| Local LLM insights | Yes | Yes | Yes | Yes |
| Cloud LLM insights | - | Yes | Yes | Yes |
| Cloud sync | - | Yes | Yes | Yes |
| Team aggregates | - | - | Yes | Yes |
| Custom rules | - | Yes | Yes | Yes |
