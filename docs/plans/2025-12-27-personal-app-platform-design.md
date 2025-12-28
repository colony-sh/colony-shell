# Colony: Personal App Platform

**Date:** 2025-12-27  
**Status:** Design Complete  
**Author:** Brainstorming session

---

## Executive Summary

Colony evolves from a developer-focused AI-native shell into a **personal app development platform** that enables non-technical users to interact with their digital world naturally. Users describe what they want in conversation; Colony generates custom software on demand.

The core insight: **The future of personal computing isn't apps you download—it's software generated on demand for an audience of one.**

---

## 1. Product Vision

### 1.1 The One-Liner

A terminal-based conversational environment where anyone can describe what they want and get custom software built on demand.

### 1.2 The Problem

Non-technical people are surrounded by their digital world—files, emails, spreadsheets, calendars, apps—but can't make it work *for* them. They're stuck with pre-built tools that don't fit their needs, or dependent on technical people to build custom solutions.

### 1.3 The Solution

A conversational interface (terminal-first) where you describe what you want in natural language. An AI generates the code to make it happen—pulling data from anywhere, transforming it, displaying it, automating it. The code is invisible unless you want to see it. Successful patterns become reusable recipes that can run on demand, on schedule, or as live dashboard widgets.

### 1.4 Target User

People who have data and workflows they want to manage, but don't know how to code. They're comfortable typing (not afraid of a text interface), but expect natural language to work.

### 1.5 Key Differentiator

Colony doesn't ship 500 integrations. Colony **writes** the integration when you need it. Custom software, built on demand, for an audience of one.

---

## 2. Colony Modes

Colony is one product with two modes, sharing the same underlying infrastructure.

### 2.1 Terminal+ Mode (Default for Technical Users)

- Full shell with POSIX compliance
- Command line, pipelines, scripting
- Agent assistance available but not primary
- The Colony that exists today

### 2.2 Personal App Mode (For Non-Technical Users)

- Conversational interface takes over
- Two-pane layout (conversation + workspace)
- Natural language primary, code invisible
- The new vision

### 2.3 Mode Switching

**At launch:**
```bash
colony --mode personal    # Launch in personal mode
colony --mode terminal    # Launch in terminal mode (default)
```

**Within a session:**
```
/personal    # Switch to personal app mode
/terminal    # Switch back to terminal mode
```

### 2.4 Shared Foundation

Both modes use the same capabilities:
- Same code generation engine
- Same data connectivity
- Same tuple space for state
- Same recipe storage

The difference is presentation and interaction style, not capability. A power user in terminal mode can run recipes a non-technical user built in personal mode.

---

## 3. Interaction Model

### 3.1 Interface Layout

**Two-pane terminal:**
- **Left pane: Conversation** — Dialogue with Colony. Natural language in, responses + status out.
- **Right pane: Workspace** — Artifacts materialize here. Charts, tables, widgets, dashboards.

User can toggle between conversation-focused (workspace collapsed) and workspace-focused (conversation collapsed) views.

### 3.2 Conversation Flow

**User speaks naturally:**
```
"Show me how much I spent on food this month"
```

**Colony responds conversationally:**
```
I'll check your bank transactions. Which account should I look at?
- Chase checking
- Amex credit card  
- Both
```

**User answers:**
```
Both
```

**Colony explains what it'll do:**
```
I'll pull transactions from Chase and Amex for December, 
filter for food/dining/groceries, and show you a breakdown. 
This will access your Plaid connection. OK?
```

**User confirms:**
```
Yes
```

**Colony executes, artifact appears in workspace:**
A chart showing food spending by category, with total.

### 3.3 Progressive Disclosure of Code

| User Action | What They See |
|-------------|---------------|
| Default | Results only |
| "How did you do that?" | Plain-English explanation |
| "Show me the code" | Generated code revealed |
| "Let me edit this" | Code becomes editable |

### 3.4 Learning Support

- **Just talk** — Colony interprets natural language, asks for clarification when needed
- **Inline suggestions** — "Did you mean..." or "You could also ask me to..."
- **Phrasebook** — Accessible reference of example prompts organized by category

---

## 4. Recipes & Artifacts

### 4.1 What is a Recipe?

A recipe is generated code that Colony saves for reuse. When Colony builds something that works, it becomes a recipe automatically.

**Recipe properties:**
- **Name** — Human-readable ("my food spending", "morning briefing")
- **Code** — The generated shell/TS/Python
- **Inputs** — Parameters it needs (date range, account, etc.)
- **Output type** — What it produces (chart, table, number, notification)
- **History** — When it ran, what it produced

### 4.2 Recipe Lifecycle

```
Conversation → Generated Code → Execution → Success → Recipe Created
                                    ↓
                               Failure → Colony fixes → Retry
```

### 4.3 Using Recipes

**Invoke manually (Personal App Mode):**
```
"Run my expense tracker"
"Show food spending again"
```

**Invoke manually (Terminal Mode):**
```bash
colony run expense-tracker --month december
```

**Schedule:**
```
"Run this every Monday morning"
```

**Embed as widget:**
```
"Keep this on my dashboard"
```

### 4.4 Artifacts

An artifact is a composed thing made of one or more recipe outputs:

- **Dashboard** — Multiple widgets arranged together
- **Report** — Generated document with embedded data
- **Automation** — Triggered workflow (when X happens, do Y)

### 4.5 Artifact Graduation

1. **Live in workspace** — Exists in current session
2. **Save as Colony document** — Persisted, reopenable
3. **Export as native app** (future) — macOS widget, menubar app, mobile widget

---

## 5. Data & Connectivity

### 5.1 The Principle

Colony doesn't ship integrations. Colony **writes** integrations on demand.

When a user says "show me my bank balance," Colony:
1. Figures out how to get that data
2. Generates the code to fetch it
3. Runs it, shows results
4. Saves the recipe for next time

### 5.2 Data Source Categories

| Category | Examples | How Colony Connects |
|----------|----------|---------------------|
| Local files | CSV, JSON, documents | Direct filesystem access |
| Cloud files | Google Drive, Dropbox | OAuth + generated API code |
| Structured services | Google Sheets, Airtable, Notion | Generated API code |
| Financial | Banks, credit cards | Plaid or similar aggregator |
| Communication | Email, calendar | OAuth + generated API code |
| Web | Any website | Generated scraping code |
| APIs | Any public/private API | Generated code from docs/schema |

### 5.3 First-Time Connection Flow

**User:** "What meetings do I have tomorrow?"

**Colony:** 
```
I don't have access to your calendar yet. I can connect to:
- Google Calendar
- Outlook/Microsoft 365
- Apple Calendar (local)

Which do you use?
```

**User:** "Google"

**Colony:**
```
I'll open Google to ask for calendar access. 
This lets me read your events (not modify them). OK?
```

*OAuth flow happens*

**Colony:**
```
Connected. You have 3 meetings tomorrow: [shows results]
```

### 5.4 Credential Storage

- Credentials stored locally (system keychain)
- User owns their auth tokens
- Colony never sends credentials to external servers (except the service itself)

### 5.5 Generated Code Language Selection

| Task Type | Language | Rationale |
|-----------|----------|-----------|
| File manipulation | Shell | POSIX tools excel here |
| API calls | TypeScript | Good async, typed, sandboxed |
| Data analysis | Python | Pandas, numpy ecosystem |
| Web scraping | Python or TS | BeautifulSoup / Puppeteer |
| System automation | Shell | Native OS integration |

User never thinks about this—it's an implementation detail.

---

## 6. Shared Pattern Library

### 6.1 The Concept

When Colony generates code that works for a common task, that pattern can be anonymized and shared to a central library. Next time anyone asks for the same thing, Colony uses the proven pattern.

### 6.2 Contributing (Opt-in)

1. Colony generates code that succeeds
2. Colony recognizes it's a generalizable pattern
3. Prompts: "This could help other Colony users. Share anonymously?"
4. If yes: Code structure uploaded, stripped of personal data

### 6.3 Consuming (Automatic)

1. User asks for something
2. Colony checks pattern library first
3. If match: Use proven pattern (faster, more reliable)
4. If no match: Generate fresh, potentially contribute back

### 6.4 What Gets Shared vs. Protected

| Shared | NOT Shared |
|--------|------------|
| Code structure/logic | Credentials |
| API endpoints used | Personal data |
| Error handling patterns | User-specific parameters |
| Data transformation logic | File paths |

### 6.5 Trust Model

- Patterns validated before entering library
- Users can opt out entirely
- Users can inspect any pattern before use
- Local-only mode available

---

## 7. Trust & Safety

### 7.1 Three-Layer Safety Model

**Layer 1: Explain Before Execute**

Colony describes what it's about to do in plain English before doing it.

```
User: "Delete all my old receipts"

Colony: "I'll delete 47 PDF files in ~/Documents/Receipts 
        that are older than 1 year. This can't be undone.
        
        Should I:
        - Show you the list first
        - Move them to trash instead
        - Delete them permanently"
```

**Layer 2: Audit Trail**

Everything logged:
- What was requested
- What code was generated
- What executed
- What changed

User can ask "What have you done today?" for a readable summary.

**Layer 3: Graduated Trust**

| Trust Level | Behavior | Example |
|-------------|----------|---------|
| New | Confirm everything | "Can I read your Google Sheets?" |
| Familiar | Confirm writes, not reads | Reads silently, asks before editing |
| Trusted | Confirm destructive only | Edits silently, asks before deleting |

Trust is per-source, per-action. User can reset anytime.

### 7.2 Failure Handling

1. **Automatic recovery** — Transient failures retry silently
2. **Plain English errors** — "Google Sheets isn't responding. Try again?"
3. **Suggested alternatives** — "I can't reach your bank. Use last week's data?"
4. **Graceful degradation** — Show what worked, explain what didn't

### 7.3 Permissions Visibility

```
User: "What access do you have?"

Colony: "Here's what I can currently access:
        
        ✓ Google Calendar (read only)
        ✓ Google Sheets (read and write)
        ✓ Chase Bank via Plaid (read only)
        ✓ Local files in ~/Documents
        ✗ Email (not connected)
        
        Want to change any of these?"
```

---

## 8. Technical Architecture

### 8.1 Building on Existing Infrastructure

| Existing Capability | How Personal Mode Uses It |
|---------------------|---------------------------|
| POSIX shell + execution | Runs generated shell scripts |
| rustyscript/Deno sandbox | Runs generated TypeScript safely |
| Multi-LLM router | Powers conversation + code generation |
| MCP integration | Structured tool access when available |
| Tuple space | State coordination, recipe storage |
| OSC 2337 protocol | Renders artifacts in terminal UI |
| Channels (fd 3-6) | Context, logging, tracing |
| .colonyrc / trust system | Permissions, configuration |

### 8.2 New Components Needed

| Component | Purpose |
|-----------|---------|
| Personal Mode UI | Two-pane conversation + workspace |
| Code Generator | LLM prompting for shell/TS/Python |
| Recipe Manager | Store, retrieve, version recipes |
| Artifact Renderer | Charts, tables, widgets in terminal |
| Connection Manager | OAuth flows, credential storage |
| Pattern Library Client | Fetch/contribute shared patterns |
| Scheduler | Cron-like triggers for recipes |
| Trust Tracker | Per-source, per-action permission state |

### 8.3 Code Generation Pipeline

```
┌─────────────────────────────────────────────────────────────┐
│                     User Request                            │
│              "Show my food spending this month"             │
└─────────────────────┬───────────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                   Pattern Library Check                     │
│         Is there a proven pattern for this task?            │
└─────────────────────┬───────────────────────────────────────┘
                      ▼
        ┌─────── Yes ─┴─ No ───────┐
        ▼                          ▼
┌───────────────┐     ┌────────────────────────────────────┐
│ Use existing  │     │         LLM Code Generation        │
│   pattern     │     │  - Determine language              │
└───────┬───────┘     │  - Generate code                   │
        │             │  - Include error handling          │
        │             └─────────────┬──────────────────────┘
        └───────────────┬───────────┘
                        ▼
┌─────────────────────────────────────────────────────────────┐
│                    Explain to User                          │
│     "I'll pull transactions from Plaid and categorize       │
│      them. This accesses your Chase account. OK?"           │
└─────────────────────┬───────────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                      Execute                                │
│           Run in appropriate sandbox/runtime                │
└─────────────────────┬───────────────────────────────────────┘
                      ▼
        ┌─────── Success ─┴─ Failure ──┐
        ▼                              ▼
┌───────────────────┐     ┌────────────────────────────────┐
│  Render artifact  │     │     LLM diagnoses + fixes      │
│  Save as recipe   │     │     Retry (up to N times)      │
│  Offer to share   │     └────────────────────────────────┘
└───────────────────┘
```

### 8.4 Runtime Sandboxing

| Language | Runtime | Sandbox |
|----------|---------|---------|
| Shell | colony-exec | Process isolation, restricted paths |
| TypeScript | rustyscript (Deno) | V8 isolate, explicit permissions |
| Python | Subprocess | venv, restricted imports (future: WASM) |

---

## 9. Implementation Phases

### Phase 0: Foundation (Prep Work)

**Goal:** Ensure existing infrastructure is ready.

- [ ] Audit LLM integration for conversational code generation
- [ ] Verify rustyscript sandbox is production-ready
- [ ] Ensure OSC 2337 can render rich artifacts
- [ ] Design recipe storage schema

**Exit:** Can manually generate and execute code via LLM, render output.

### Phase 1: Conversational Core

**Goal:** Natural language → code → execution loop works.

- [ ] Personal Mode entry point
- [ ] Two-pane UI scaffold
- [ ] Code generation prompts for shell/TS/Python
- [ ] Explain-before-execute flow
- [ ] Basic artifact rendering

**Exit:** User asks "list files in Documents" and sees results.

### Phase 2: Recipes & Persistence

**Goal:** Successful executions become reusable.

- [ ] Recipe storage
- [ ] Recipe invocation by name
- [ ] Recipe listing and management
- [ ] Execution history and audit trail

**Exit:** User can build something, name it, run it again tomorrow.

### Phase 3: Data Connectivity

**Goal:** Connect to real-world data sources.

- [ ] OAuth flow infrastructure
- [ ] Credential storage (system keychain)
- [ ] Local file access with permission prompts
- [ ] First integrations: Google Sheets, local CSV, one API

**Exit:** User asks "what's on my calendar tomorrow" with real data.

### Phase 4: Rich Artifacts

**Goal:** Workspace shows compelling visualizations.

- [ ] Chart rendering (bar, line, pie)
- [ ] Interactive tables
- [ ] Dashboard layout (multiple widgets)
- [ ] Artifact persistence

**Exit:** User can build a personal dashboard with live widgets.

### Phase 5: Scheduling & Automation

**Goal:** Recipes run on triggers.

- [ ] Cron-like scheduler
- [ ] Notification system
- [ ] Event triggers
- [ ] Natural language automation rules

**Exit:** User says "every Monday, email me my spending summary."

### Phase 6: Pattern Library

**Goal:** Community patterns accelerate everyone.

- [ ] Pattern anonymization and extraction
- [ ] Central pattern service
- [ ] Pattern matching and retrieval
- [ ] Contribution opt-in flow

**Exit:** Common tasks use proven patterns, not fresh generation.

### Phase 7: Polish & Native Export

**Goal:** Production-ready, artifact graduation.

- [ ] Graduated trust fully implemented
- [ ] Error handling polished
- [ ] Performance optimization
- [ ] Export to Colony document format
- [ ] (Stretch) Export to native widget

**Exit:** Non-technical user can build and maintain a dashboard without help.

### Phase Sequencing

```
Phase 0 ──▶ Phase 1 ──▶ Phase 2 ──▶ Phase 3
                              │
                              ▼
              Phase 4 ◀── Phase 5 ──▶ Phase 6
                              │
                              ▼
                          Phase 7
```

Phases 4, 5, 6 can partially parallelize after Phase 3.

---

## 10. Success Metrics

### 10.1 User Success

- Time to first working artifact (target: < 5 minutes)
- Recipe reuse rate (target: > 50% of executions use saved recipes)
- User retention at 30 days

### 10.2 Technical Success

- Code generation success rate (target: > 80% first try)
- Pattern library hit rate (target: > 30% of requests)
- Execution reliability (target: > 95% uptime for scheduled recipes)

### 10.3 Safety Success

- Zero credential leaks
- < 1% of executions cause unintended data changes
- User-reported trust incidents < 0.1%

---

## 11. Open Questions

### 11.1 Deferred Decisions

- Product naming (Colony? New brand for Personal Mode?)
- Mobile app strategy (PWA vs native companion app)
- Pattern library moderation (automated vs human review)
- Enterprise sales motion (self-serve vs sales-led)

### 11.2 Technical Unknowns

- Python sandboxing approach (subprocess vs WASM vs container)
- Cloud execution provider (build vs buy: Cloudflare/Fly.io/Railway)
- Credential vault implementation (HashiCorp Vault vs AWS Secrets Manager vs custom)
- Real-time sync protocol for team workspaces

### 11.3 Research Needed

- User testing with actual non-technical users
- LLM cost optimization for code generation (caching, smaller models)
- Optimal cloud execution pricing (cost modeling at scale)
- Enterprise buyer interviews for feature prioritization

---

## 12. Competitive Landscape

### 12.1 Market Categories Analyzed

| Category | Players | Colony's Position |
|----------|---------|-------------------|
| Automation platforms | Zapier, IFTTT, Make, n8n | No pre-built connector limits |
| Internal tool builders | Retool, Appsmith | Single-user focus, no SSO complexity |
| Productivity databases | Notion, Airtable | No arbitrary limits, local-first |
| AI-native tools | Bolt.new, v0, Lovable | Polyglot generation, recipes persist |
| Personal dashboards | Geckoboard, Monarch | Universal, not vertical-specific |

### 12.2 Automation Platforms (Zapier, IFTTT, Make, n8n)

| Platform | Target | Pricing Model | Key Limitation |
|----------|--------|---------------|----------------|
| Zapier | Business users | Per-task ($20-70/mo) | Expensive at scale, 1.5★ Trustpilot |
| IFTTT | Consumers | Per-applet ($3-9/mo) | Gutted free tier, 1.5★ Trustpilot |
| Make.com | Power users | Per-credit ($9-29/mo) | No self-hosting, AI costs variable |
| n8n | Developers | Per-execution ($0-50/mo) | Steep learning curve |

**User Pain Points:**
- Cost unpredictability (task/operation-based pricing surprises users)
- Reliability (automations fail silently, triggers don't fire)
- Vendor lock-in (can't export workflows)
- Integration limits (pre-built connectors only)

**Colony Advantage:** Code generation means no pre-built connector limits. Any API, any website, any data source.

### 12.3 Internal Tool Builders (Retool, Appsmith)

| Platform | Target | Pricing | Key Limitation |
|----------|--------|---------|----------------|
| Retool | Enterprise dev teams | $10-50/user/mo | SSO requires Enterprise, no code export |
| Appsmith | Cost-conscious devs | $0-25/user/mo | No mobile apps, performance issues |

**User Pain Points:**
- Pricing tiers lock essential features (SSO, Git integration require expensive plans)
- Performance degrades with complex apps
- Still requires technical users ("low-code" but need database knowledge)

**Colony Advantage:** Single-user focus means no SSO complexity. Code generation handles technical complexity invisibly.

### 12.4 Productivity Databases (Notion, Airtable)

| Platform | Target | Pricing | Key Limitation |
|----------|--------|---------|----------------|
| Notion | Knowledge workers | $12-20/user/mo | Performance issues, limited offline |
| Airtable | Operations teams | $20-45/user/mo | Hard record limits (50K), no offline |

**User Pain Points:**
- Record limits (Airtable's 50K cap breaks businesses)
- Performance (large databases become sluggish)
- Learning curve (flexibility becomes overwhelming)
- No offline (dependent on internet)

**Colony Advantage:** Local-first means no arbitrary limits. Terminal-based means true offline capability.

### 12.5 AI-Native Tools (Bolt.new, v0, Lovable)

| Platform | Type | Pricing | Key Limitation |
|----------|------|---------|----------------|
| Bolt.new | Full-stack generator | $0-20/mo | Burns tokens fixing bugs, 1.5★ Trustpilot |
| v0 by Vercel | UI component generator | $0-20/mo | Frontend only, no backend |
| Lovable | Full-stack MVP builder | $0-25/mo | Supabase lock-in |

**User Pain Points:**
- Token consumption (2M+ tokens just fixing bugs)
- "Demo to production" gap (works in demos, breaks in real use)
- Debugging difficulty (AI-generated code you don't understand)
- Backend limitations (toy backends you can't migrate)

**Colony Advantage:** Polyglot generation picks the right tool. Recipes mean you don't regenerate from scratch. Peek-under-hood enables learning.

### 12.6 Personal Dashboards (Geckoboard, Monarch)

| Platform | Target | Pricing | Key Limitation |
|----------|--------|---------|----------------|
| Geckoboard | SMB metrics | $75/mo | Limited to pre-built integrations |
| Monarch Money | Personal finance | $15/mo | Finance-only |
| Looker Studio | Budget-conscious | Free | Requires data modeling knowledge |

**User Pain Points:**
- Pre-built integrations only (can't connect custom sources)
- Vertical-specific (finance OR metrics, not both)
- Learning curve (even "easy" tools require understanding data sources)

**Colony Advantage:** Generate integrations on demand. Dashboard = recipe outputs composed. Not vertical-specific.

### 12.7 Colony's Unique Position

| Competitor Weakness | Colony Advantage |
|---------------------|------------------|
| Pre-built integrations only | Code generation on demand |
| Per-task/operation pricing | Execution-based (your compute) |
| Vendor lock-in | Local-first, code visible |
| Cloud-only | Terminal = offline capable |
| GUI complexity | Conversational interface |
| Vertical-specific | Universal (anything you describe) |
| Toy backends | Polyglot generation (real code) |

### 12.8 Competitive Positioning Statement

> Colony is not another no-code platform. It's a personal software factory that generates exactly what you need through conversation—no pre-built integrations to configure, no arbitrary limits, no vendor lock-in. Your recipes run locally on your machine with your data.

---

## 13. Monetization Strategy

### 13.1 Pricing Philosophy

**Core principles:**
- **Local-first is free** — Running recipes on your own machine costs Colony nothing
- **Cloud features have real costs** — Remote execution, pattern library, team sync
- **Value-aligned pricing** — Pay for what you use, not arbitrary seat counts
- **No vendor lock-in** — Export recipes, own your data, self-host if desired

### 13.2 Pricing Model: Hybrid (Base + Usage)

Based on research showing hybrid models have 21% median growth rate and 27% lower churn than flat-rate:

| Tier | Price | What's Included |
|------|-------|-----------------|
| **Free** | $0 | Local execution, unlimited recipes, pattern library (read), basic LLM quota |
| **Pro** | $15/month | Cloud execution credits, scheduled recipes, pattern contributions, priority LLM |
| **Team** | $50/month + $10/seat | Shared recipes, team workspace, audit logs, SSO |
| **Enterprise** | Custom | Self-hosted, SLA, dedicated support, compliance features |

### 13.3 Usage-Based Components

| Resource | Free | Pro | Team |
|----------|------|-----|------|
| Local recipe executions | Unlimited | Unlimited | Unlimited |
| Cloud executions | 0 | 1,000/month | 5,000/month + overage |
| LLM tokens (code generation) | 50K/month | 500K/month | 2M/month + overage |
| Pattern library contributions | Read-only | Full access | Full access |
| Scheduled recipes | 0 | 10 | Unlimited |
| Connected data sources | 3 | Unlimited | Unlimited |

**Overage rates:**
- Cloud executions: $0.01 per execution
- LLM tokens: $0.002 per 1K tokens

### 13.4 Why This Model Works

| Design Choice | Rationale |
|---------------|-----------|
| Free local execution | Drives adoption, zero marginal cost to Colony |
| Cloud execution paid | Real infrastructure cost, enables mobile/scheduled |
| LLM quota | AI costs are significant, aligns with actual value |
| Pattern library free read | Network effect, community growth |
| Team features separate | Different buyer (manager vs individual) |

### 13.5 Conversion Strategy

**Free tier limitations that trigger upgrade:**
- Hit LLM quota while building something important
- Want recipes to run when laptop is closed (→ cloud execution)
- Need more than 3 data sources connected
- Team member wants to use your recipe (→ Team tier)

**Target conversion timeline:** 1-3 months (matches "quicksand effect" research)

---

## 14. Remote Execution (Colony Cloud)

### 14.1 Why Remote Execution?

Local-first is the default, but remote execution enables:
- **Scheduled recipes** — Run "every Monday morning" without laptop open
- **Mobile access** — View dashboards from phone (future)
- **Webhooks/triggers** — React to external events instantly
- **Heavy compute** — GPU workloads, large data processing
- **Team sharing** — Recipes run in shared environment

### 14.2 Architecture Options

| Approach | Startup | Security | Cost | Best For |
|----------|---------|----------|------|----------|
| **V8 Isolates** (Cloudflare/Deno) | <5ms | Good | Low | Simple TS recipes |
| **Firecracker microVMs** | ~300ms | Excellent | Medium | Untrusted code, polyglot |
| **Containers** | 100ms-2s | Good | Medium | Complex dependencies |

**Recommended: Hybrid approach**
- V8 isolates for TypeScript recipes (fast, cheap)
- Firecracker microVMs for shell/Python (secure, flexible)

### 14.3 Execution Model

```
┌─────────────────────────────────────────────────────────────┐
│                    User's Local Colony                       │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Recipe: "morning-briefing"                          │    │
│  │  Schedule: "0 7 * * 1-5" (weekdays 7am)             │    │
│  │  Execution: cloud                                    │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────┬───────────────────────────────────────┘
                      │ Sync recipe + schedule
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                    Colony Cloud                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │  Scheduler  │  │  Executor   │  │  Credential │         │
│  │             │──│  (Sandbox)  │──│   Vault     │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
│                          │                                   │
│                          ▼                                   │
│                   ┌─────────────┐                           │
│                   │   Results   │                           │
│                   │   Storage   │                           │
│                   └─────────────┘                           │
└─────────────────────┬───────────────────────────────────────┘
                      │ Push notification / sync results
                      ▼
┌─────────────────────────────────────────────────────────────┐
│              User's Phone / Next Session                     │
└─────────────────────────────────────────────────────────────┘
```

### 14.4 Security Model

**Credential handling:**
- User's OAuth tokens stored in Colony Cloud vault (encrypted at rest)
- Tokens never leave vault except to sandbox during execution
- User can revoke cloud access anytime
- Audit log of all cloud executions

**Code sandboxing:**
- Each execution in isolated environment
- No network access except explicitly allowed endpoints
- Resource limits (CPU time, memory, execution duration)
- No persistent filesystem between executions

### 14.5 Pricing Rationale

Cloud execution has real costs:
- Compute time (~$0.000017/GB-second on Lambda)
- Cold starts and warm instance maintenance
- Credential vault infrastructure
- Bandwidth for results

**$0.01/execution** covers:
- Average 500ms execution at 512MB = ~$0.000008 compute
- Margin for infrastructure, cold starts, failures
- Sustainable at scale

---

## 15. Teams & Organizations

### 15.1 Evolution from Single-User

Colony starts single-user but naturally expands:
1. **Individual** builds useful recipes
2. **Shares recipe** with colleague ("you should try this")
3. **Team** wants shared dashboards, consistent data
4. **Organization** needs governance, audit, SSO

### 15.2 Team Features

| Feature | Purpose |
|---------|---------|
| **Shared recipe library** | Team members use each other's recipes |
| **Shared workspace** | Collaborative dashboards, widgets |
| **Shared connections** | Team OAuth tokens (e.g., company Google Workspace) |
| **Activity feed** | See what teammates are building |
| **Permissions** | Who can edit vs run vs view |

### 15.3 Team Pricing Model

**Base + seats (not pure per-seat):**
- $50/month base (includes 3 seats)
- $10/month per additional seat
- Shared cloud execution pool (5,000/month)
- Shared LLM token pool (2M/month)

**Why not pure per-seat:**
- Avoids "seat counting" friction
- Teams have mix of heavy and light users
- Usage pools encourage collaboration

### 15.4 Seat Types

| Seat Type | Can Do | Price |
|-----------|--------|-------|
| **Full** | Create, edit, run recipes, manage connections | $10/month |
| **Runner** | Run existing recipes, view dashboards | $5/month |
| **Viewer** | View dashboards only | Free |

Based on research: differentiating seats reduces friction for stakeholders who only need limited access.

### 15.5 Enterprise Features

| Feature | Why Enterprises Need It |
|---------|------------------------|
| **SSO/SAML** | Centralized identity, compliance |
| **SCIM provisioning** | Automated user management |
| **Audit logs** | Security investigations, compliance |
| **Data residency** | GDPR, sovereignty requirements |
| **Self-hosted option** | Air-gapped environments, full control |
| **SLA** | Guaranteed uptime for critical workflows |
| **Dedicated support** | Named account manager |

**Enterprise pricing:** Custom, typically $25K-100K/year based on seats and usage.

### 15.6 Team Implementation Phases

**Phase 1 (MVP):** Recipe sharing via export/import
**Phase 2:** Shared workspace with permissions
**Phase 3:** Team connections and credential sharing
**Phase 4:** SSO, audit logs, enterprise features

---

## 16. Appendix: Boundaries

### 16.1 V1 Scope

- **In:** Single-user personal tools
- **In:** Desktop terminal experience
- **In:** Data from everywhere (files, APIs, web)
- **In:** Code invisible by default, peekable

### 16.2 Future Scope

- **Phase 2:** Remote execution (Colony Cloud)
- **Phase 3:** Teams and shared workspaces
- **Later:** Mobile native apps
- **Later:** Enterprise features (SSO, audit, compliance)
- **Later:** Marketplace for recipes/patterns

### 16.3 Non-Goals

- Replacing professional development tools
- Competing with Zapier/IFTTT directly (different UX model)
- Building a no-code GUI builder (terminal-first always)
