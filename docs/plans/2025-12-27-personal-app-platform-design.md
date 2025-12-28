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

## 16. Infrastructure & Deployment Modes

Colony adapts to its environment—from a single user's laptop to a Kubernetes cluster serving an organization.

### 16.1 Configurable Database Backend

**Default: SQLite** — Zero configuration, single file, perfect for personal use.

**Upgrade path via built-in command:**

```bash
# Check current database
colony db status
# Database: SQLite
# Location: ~/.colony/colony.db
# Size: 12.4 MB
# Recipes: 47

# Install and configure PostgreSQL
colony db install postgres
# This will:
# - Install PostgreSQL (via Homebrew/apt/docker)
# - Create colony database and user
# - Migrate existing data from SQLite
# - Update Colony configuration
# Proceed? [y/N]

# Or connect to existing PostgreSQL
colony db connect postgres://user:pass@localhost:5432/colony

# Switch to MySQL
colony db install mysql
colony db connect mysql://user:pass@localhost:3306/colony
```

**Database comparison:**

| Backend | Best For | Concurrent Users | Setup |
|---------|----------|------------------|-------|
| **SQLite** | Personal, single user | 1 | Zero config |
| **PostgreSQL** | Teams, self-hosted | 100+ | `colony db install postgres` |
| **MySQL** | Enterprise, existing infra | 100+ | `colony db install mysql` |

**Migration is automatic:**
- Colony detects schema version
- Runs migrations on startup
- Handles SQLite → PostgreSQL data transfer
- Zero downtime for teams (run migration, then switch connection)

### 16.2 Built-in HTTP Server

Colony can expose dashboards and artifacts via HTTP—no external web server needed.

```bash
# Start HTTP server for current workspace
colony serve
# Server running at http://localhost:3000
# Serving: 3 dashboards, 12 widgets
# Press Ctrl+C to stop

# Configure port and host
colony serve --port 8080 --host 0.0.0.0

# Serve specific dashboard
colony serve --dashboard "sales-metrics"

# Enable authentication
colony serve --auth basic    # Username/password
colony serve --auth token    # API token
colony serve --auth sso      # SSO (Team/Enterprise)

# Production mode with TLS
colony serve --tls --cert /path/to/cert.pem --key /path/to/key.pem
```

**HTTP server capabilities:**

| Feature | Description |
|---------|-------------|
| **Dashboard serving** | Render dashboards as web pages |
| **Widget API** | JSON endpoints for widget data |
| **Webhook receiver** | Trigger recipes from external events |
| **Health checks** | `/health` and `/ready` endpoints |
| **Metrics** | Prometheus-compatible `/metrics` endpoint |
| **WebSocket** | Real-time dashboard updates |

**Use cases:**
- Share dashboard with team via URL
- Embed widgets in other tools (iframe)
- Receive webhooks from GitHub, Stripe, etc.
- Monitoring and alerting integration

### 16.3 Docker Mode

Colony becomes a conversational interface to Docker, letting users manage containers without memorizing commands.

**Entering Docker mode:**
```bash
colony --mode docker
# Or within a session:
/docker
```

**Docker Sidebar:**

When in Docker mode, a sidebar appears showing your Docker environment at a glance:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ┌─────────────────────┐                                                 │
│ │ 🐳 DOCKER           │  Conversation                                   │
│ │                     │  ─────────────────────────────────────────────  │
│ │ ▼ Containers (4)    │                                                 │
│ │   ● postgres-dev    │  User: "Why is the API container restarting?"   │
│ │   ● redis-cache     │                                                 │
│ │   ● api-server  ⟳   │  Colony: Looking at api-server logs...          │
│ │   ● nginx-proxy     │                                                 │
│ │                     │  The container is OOM-killed. Last 3 restarts:  │
│ │ ▼ Images (12)       │  - 14:23 OOMKilled (512Mi limit exceeded)       │
│ │   node:20-alpine    │  - 14:18 OOMKilled                              │
│ │   postgres:16       │  - 14:12 OOMKilled                              │
│ │   redis:7-alpine    │                                                 │
│ │   myapp:latest      │  Current memory: 489Mi / 512Mi limit            │
│ │   myapp:v2.3.0      │                                                 │
│ │   +7 more...        │  Recommendation: Increase memory limit to 1Gi   │
│ │                     │                                                 │
│ │ ▼ Volumes (3)       │  ─────────────────────────────────────────────  │
│ │   postgres_data     │  > _                                            │
│ │   redis_data        │                                                 │
│ │   app_uploads       │                                                 │
│ │                     │                                                 │
│ │ ▼ Networks (2)      │                                                 │
│ │   colony-net        │                                                 │
│ │   bridge            │                                                 │
│ │                     │                                                 │
│ │ ─────────────────── │                                                 │
│ │ CPU:  23% ████░░░░  │                                                 │
│ │ MEM:  4.2 / 16 GB   │                                                 │
│ │ DISK: 12.3 GB used  │                                                 │
│ └─────────────────────┘                                                 │
└─────────────────────────────────────────────────────────────────────────┘
```

**Sidebar features:**

| Section | Shows | Interactions |
|---------|-------|--------------|
| **Containers** | Running/stopped containers with status indicators | Click to select, right-click for actions |
| **Images** | Local images sorted by recent use | Click to run, inspect layers |
| **Volumes** | Named volumes with size | Click to browse contents |
| **Networks** | Docker networks | Click to see connected containers |
| **Resources** | CPU/Memory/Disk usage | Real-time updates |

**Status indicators:**
- `●` Running healthy
- `⟳` Restarting / unhealthy
- `○` Stopped
- `◐` Starting

**Keyboard navigation:**
- `Tab` - Switch focus between sidebar and conversation
- `↑↓` - Navigate sidebar items
- `Enter` - Select/expand item
- `d` - Delete selected
- `l` - View logs
- `s` - Start/stop toggle
- `i` - Inspect details

**Conversational container management:**

```
User: "What containers are running?"

Colony: You have 4 containers running:
        
        CONTAINER        IMAGE              STATUS      PORTS
        postgres-dev     postgres:16        Up 2 days   5432→5432
        redis-cache      redis:7            Up 2 days   6379→6379
        api-server       myapp:latest       Up 4 hours  3000→3000
        nginx-proxy      nginx:alpine       Up 4 hours  80→80, 443→443

User: "Stop the api server and show me its logs from the last hour"

Colony: I'll stop api-server and fetch recent logs. OK?

User: "Yes"

Colony: [Stops container]
        
        Last hour of logs from api-server:
        
        14:23:01 INFO  Server starting on port 3000
        14:23:02 INFO  Connected to database
        14:45:17 WARN  Slow query detected (2.3s): SELECT * FROM orders...
        15:02:33 ERROR Connection refused to redis-cache:6379
        15:02:34 INFO  Reconnected to Redis
        ...
        
        Container stopped. Want me to investigate that Redis connection issue?
```

**Docker recipes:**

```
User: "Every time I start working, I need postgres and redis running"

Colony: I'll create a recipe called "dev-environment" that:
        - Starts postgres:16 on port 5432
        - Starts redis:7 on port 6379
        - Waits for both to be healthy
        - Shows you the connection strings
        
        Save this recipe?

User: "Yes"

Colony: Saved. Next time just say "start my dev environment" or run:
        colony run dev-environment
```

**Docker mode capabilities:**

| Natural Language | Docker Equivalent |
|------------------|-------------------|
| "What's running?" | `docker ps` |
| "Show logs for X" | `docker logs X` |
| "Stop everything" | `docker stop $(docker ps -q)` |
| "Clean up old images" | `docker system prune` |
| "Run postgres" | `docker run -d postgres:16...` |
| "Why did X crash?" | `docker logs X --tail 100` + analysis |
| "How much space is Docker using?" | `docker system df` |
| "Connect me to the database container" | `docker exec -it postgres psql` |

**Compose integration:**

```
User: "Start my project"

Colony: I found docker-compose.yml in this directory. It defines:
        - api (Node.js app)
        - db (PostgreSQL)
        - cache (Redis)
        
        Start all services?

User: "Just the database"

Colony: Starting db service only...
        
        PostgreSQL is ready:
        - Host: localhost
        - Port: 5432
        - Connection: postgres://postgres:postgres@localhost:5432/app
```

### 16.4 Kubernetes Mode

Colony becomes a conversational interface to Kubernetes, making cluster management accessible.

**Entering Kubernetes mode:**
```bash
colony --mode k8s
# Or within a session:
/k8s

# Connect to specific cluster
/k8s --context production
```

**Kubernetes Sidebar:**

When in Kubernetes mode, a sidebar provides hierarchical cluster navigation:

```
┌─────────────────────────────────────────────────────────────────────────┐
│ ┌─────────────────────┐                                                 │
│ │ ☸ KUBERNETES        │  Conversation                                   │
│ │                     │  ─────────────────────────────────────────────  │
│ │ Context: production │                                                 │
│ │ ▼ Switch Context    │  User: "What's wrong with the api namespace?"   │
│ │   ○ staging         │                                                 │
│ │   ● production      │  Colony: Checking api namespace...              │
│ │   ○ development     │                                                 │
│ │                     │  Found 1 issue:                                 │
│ │ ▼ Namespaces        │                                                 │
│ │   ▼ api (7/8) ⚠     │  POD                    STATUS    RESTARTS     │
│ │     ● api-server-a  │  api-server-7d4f-x2k4   CrashLoop 5 (12m ago)  │
│ │     ● api-server-b  │                                                 │
│ │     ⟳ api-server-c  │  Last error: OOMKilled                         │
│ │     ● api-worker-a  │  Memory: 509Mi / 512Mi limit                   │
│ │     ● api-worker-b  │                                                 │
│ │   ▶ default (12/12) │  ─────────────────────────────────────────────  │
│ │   ▶ monitoring (5/5)│  > increase memory to 1Gi                       │
│ │   ▶ kube-system     │                                                 │
│ │                     │                                                 │
│ │ ▼ Workloads         │                                                 │
│ │   Deployments: 8    │                                                 │
│ │   StatefulSets: 2   │                                                 │
│ │   DaemonSets: 3     │                                                 │
│ │   Jobs: 1 running   │                                                 │
│ │                     │                                                 │
│ │ ▼ Storage           │                                                 │
│ │   PVCs: 5 (42Gi)    │                                                 │
│ │   ConfigMaps: 12    │                                                 │
│ │   Secrets: 8        │                                                 │
│ │                     │                                                 │
│ │ ─────────────────── │                                                 │
│ │ Nodes: 3/3 Ready    │                                                 │
│ │ CPU:  45% █████░░░  │                                                 │
│ │ MEM:  62% ██████░░  │                                                 │
│ └─────────────────────┘                                                 │
└─────────────────────────────────────────────────────────────────────────┘
```

**Sidebar features:**

| Section | Shows | Interactions |
|---------|-------|--------------|
| **Context** | Current cluster, quick switch | Click to change cluster |
| **Namespaces** | All namespaces with pod health summary | Expand to see pods |
| **Workloads** | Deployments, StatefulSets, DaemonSets, Jobs | Click to manage |
| **Storage** | PVCs, ConfigMaps, Secrets | Click to inspect |
| **Cluster health** | Node status, resource utilization | Real-time updates |

**Health indicators:**
- `●` All pods healthy
- `⚠` Some pods unhealthy
- `⟳` Pods restarting
- `(7/8)` Ready count / desired count

**Keyboard navigation:**
- `Tab` - Switch focus between sidebar and conversation
- `↑↓` - Navigate items
- `←→` - Collapse/expand sections
- `Enter` - Select item for details
- `l` - View logs for selected pod
- `e` - Exec into selected pod
- `d` - Describe resource
- `x` - Delete (with confirmation)
- `p` - Port-forward
- `c` - Switch context

**Context-aware conversation:**

Selecting an item in the sidebar sets context for conversation:

```
[User clicks on api-server-c in sidebar]

User: "What's wrong with this?"

Colony: api-server-c (api-server-7d4f8b6c9-x2k4m) is crash-looping.
        
        Status: CrashLoopBackOff
        Restarts: 5 in the last 12 minutes
        Last exit code: 137 (OOMKilled)
        
        Memory timeline:
        - 14:23:01  256Mi (started)
        - 14:23:45  489Mi
        - 14:23:52  509Mi
        - 14:23:53  KILLED (limit: 512Mi)
        
        This pod consistently hits its memory limit.
        
        Options:
        1. Increase memory limit to 1Gi
        2. View heap dump from last run
        3. Check for memory leaks in recent commits
```

**Built-in kubectx and kubens:**

Colony includes `kubectx` and `kubens` as built-in commands—no separate installation needed.

```bash
# List available contexts
kubectx
# staging
# production  ← current
# development

# Switch context
kubectx staging
# Switched to context "staging"

# Switch to previous context
kubectx -
# Switched to context "production"

# Interactive fuzzy selection (if multiple contexts)
kubectx
# > staging
#   production
#   development
#   docker-desktop
# (type to filter, enter to select)
```

```bash
# List namespaces in current context
kubens
# api
# default  ← current
# kube-system
# monitoring

# Switch namespace
kubens api
# Active namespace is "api"

# Switch to previous namespace
kubens -
# Active namespace is "default"

# Interactive fuzzy selection
kubens
# > api
#   default
#   monitoring
# (type to filter, enter to select)
```

**Enhanced features beyond standard kubectx/kubens:**

| Feature | Standard | Colony Built-in |
|---------|----------|-----------------|
| Context switching | ✓ | ✓ |
| Namespace switching | ✓ | ✓ |
| Fuzzy search | Requires fzf | Built-in |
| Previous context (`-`) | ✓ | ✓ |
| Context health preview | ✗ | ✓ Shows node/pod status |
| Namespace pod count | ✗ | ✓ Shows (healthy/total) |
| Sidebar sync | ✗ | ✓ Sidebar updates on switch |
| Recent contexts | ✗ | ✓ Sorted by recent use |
| Context aliases | ✗ | ✓ `kubectx prod=gke_myproj_us-east1_prod` |

**Context health preview:**

```bash
kubectx
# > staging      3 nodes, 24/24 pods ✓
#   production   5 nodes, 47/48 pods ⚠
#   development  1 node,  12/12 pods ✓
```

**Namespace pod count:**

```bash
kubens
# > api          8 pods (7 ready) ⚠
#   default      12 pods ✓
#   monitoring   5 pods ✓
#   kube-system  9 pods ✓
```

**Context aliases:**

```bash
# Create alias
kubectx prod=gke_my-project_us-east1_production
# Aliased "gke_my-project_us-east1_production" as "prod"

# Use alias
kubectx prod
# Switched to context "gke_my-project_us-east1_production"

# List aliases
kubectx --aliases
# prod → gke_my-project_us-east1_production
# stg  → gke_my-project_us-east1_staging
```

**Conversational cluster management:**

```
User: "What's the status of my cluster?"

Colony: Connected to cluster: production (GKE)
        
        NODES          STATUS    CPU     MEMORY
        node-pool-1a   Ready     45%     62%
        node-pool-1b   Ready     38%     58%
        node-pool-1c   Ready     52%     71%
        
        NAMESPACES     PODS    HEALTHY
        default        12      12 ✓
        api            8       7 ⚠ (1 CrashLoopBackOff)
        monitoring     5       5 ✓
        
        ⚠ Issue detected: 1 pod in api namespace is crash-looping.
        Want me to investigate?

User: "Yes"

Colony: Pod api-server-7d4f8b6c9-x2k4m is crash-looping.
        
        Last 3 restart reasons:
        1. OOMKilled (12 min ago) - exceeded 512Mi memory limit
        2. OOMKilled (8 min ago)
        3. OOMKilled (3 min ago)
        
        Current memory request: 256Mi, limit: 512Mi
        Peak usage before crash: 509Mi
        
        Recommendation: Increase memory limit to 1Gi
        
        Should I patch the deployment?

User: "Yes, increase it to 1Gi"

Colony: I'll update api-server deployment:
        - memory limit: 512Mi → 1Gi
        - memory request: 256Mi → 512Mi
        
        This will trigger a rolling restart. Proceed?

User: "Do it"

Colony: ✓ Deployment patched
        ✓ Rolling restart initiated
        
        Progress: 2/3 pods updated
        ...
        Progress: 3/3 pods updated
        
        All pods healthy. Memory issue resolved.
```

**Kubernetes recipes:**

```
User: "I want to check on production every morning"

Colony: I'll create a recipe "production-health-check" that:
        - Connects to production cluster
        - Checks all pod health
        - Reports any crashloops, pending pods, or resource pressure
        - Summarizes recent error logs
        - Alerts you if anything needs attention
        
        Schedule this for 8am weekdays?

User: "Yes"

Colony: Saved. You'll get a morning briefing on production health.
```

**Kubernetes mode capabilities:**

| Natural Language | Kubectl Equivalent |
|------------------|-------------------|
| "What's running in production?" | `kubectl get pods -n production` |
| "Why is the API down?" | `kubectl describe pod... + kubectl logs...` |
| "Scale up the workers" | `kubectl scale deployment workers --replicas=5` |
| "Show me recent deployments" | `kubectl rollout history deployment/...` |
| "Roll back the last deploy" | `kubectl rollout undo deployment/...` |
| "What's using the most resources?" | `kubectl top pods --sort-by=memory` |
| "Port forward to the database" | `kubectl port-forward svc/postgres 5432:5432` |
| "Show me the config for X" | `kubectl get deployment X -o yaml` |
| "Find all pods with errors" | `kubectl get pods --field-selector=status.phase!=Running` |

**Multi-cluster support:**

```
User: "Compare staging and production"

Colony: Comparing clusters...
        
        RESOURCE         STAGING    PRODUCTION    DIFF
        API replicas     2          5             ✓ Expected
        API image        v2.3.1     v2.3.0        ⚠ Staging ahead
        DB storage       10Gi       100Gi         ✓ Expected
        Worker replicas  1          10            ✓ Expected
        
        ⚠ Staging is running v2.3.1 but production has v2.3.0
        
        Want to see what changed in v2.3.1?
```

### 16.5 Mode Summary

| Mode | Activated By | Purpose |
|------|--------------|---------|
| **Terminal+** | Default | Full shell, POSIX compliance |
| **Personal App** | `--mode personal` or `/personal` | Conversational, non-technical users |
| **Docker** | `--mode docker` or `/docker` | Container management |
| **Kubernetes** | `--mode k8s` or `/k8s` | Cluster management |

Modes share the same recipe system—a Docker recipe and a Kubernetes recipe work the same way.

### 16.6 Deployment of Colony Itself

Colony can also be deployed in containers/Kubernetes for team use:

```bash
# Run Colony server in Docker
docker run -d \
  -v ~/.colony:/data \
  -p 3000:3000 \
  colony/colony:latest --mode server

# Deploy to Kubernetes via Helm
helm install colony colony/colony \
  --set database.type=postgres \
  --set replicas=3
```

See [Colony Self-Hosting Guide] for production deployment details.

### 16.7 Configuration Hierarchy

Colony configuration cascades from multiple sources:

```
Priority (highest to lowest):
1. Command-line flags        --db-url=...
2. Environment variables     COLONY_DB_URL=...
3. Local config file         ./.colony/config.toml
4. User config file          ~/.colony/config.toml
5. System config file        /etc/colony/config.toml
6. Defaults                  SQLite, port 3000, etc.
```

**Example config.toml:**

```toml
[database]
type = "postgres"
url = "postgres://colony:secret@localhost:5432/colony"
max_connections = 20

[server]
host = "0.0.0.0"
port = 3000
tls = true
cert_file = "/etc/colony/tls/cert.pem"
key_file = "/etc/colony/tls/key.pem"

[execution]
max_concurrent = 10
timeout_seconds = 300
sandbox = "firecracker"  # or "docker", "v8"

[docker]
default_network = "colony-net"
socket = "/var/run/docker.sock"

[kubernetes]
default_context = "production"
contexts = ["staging", "production"]

[auth]
type = "sso"
provider = "okta"
client_id = "..."
```

---

## 17. Appendix: Boundaries

### 17.1 V1 Scope

- **In:** Single-user personal tools
- **In:** Desktop terminal experience
- **In:** Data from everywhere (files, APIs, web)
- **In:** Code invisible by default, peekable

### 17.2 Future Scope

- **Phase 2:** Remote execution (Colony Cloud)
- **Phase 3:** Teams and shared workspaces
- **Later:** Mobile native apps
- **Later:** Enterprise features (SSO, audit, compliance)
- **Later:** Marketplace for recipes/patterns

### 17.3 Non-Goals

- Replacing professional development tools
- Competing with Zapier/IFTTT directly (different UX model)
- Building a no-code GUI builder (terminal-first always)
