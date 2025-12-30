# Colony Cloud - Initial Repository Setup Prompt

Use this prompt when initializing the `colony-cloud` repository. It provides the AI assistant with complete context about the project architecture, conventions, and session management system.

---

## Initial Prompt for New Repository

Copy everything below this line into your AI assistant when starting the colony-cloud project:

---

# Colony Cloud

Colony Cloud is a managed LLM service providing:
- REST API compatible with OpenAI's format
- Native Rust SDK (`colony-cloud-sdk`) for colony-shell integration  
- Curated models from AWS Bedrock (Claude, Llama, Mistral)
- Tiered subscriptions with included token allocations
- Recipe Registry for sharing recipes, plugins, skills, and bundles
- Privacy-first conversation memory (local default, opt-in sync)

Bedrock is the invisible backend - users see real model names but only models vetted and priced by Colony.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Clients                               │
├──────────────────┬──────────────────┬───────────────────────┤
│   colony-shell   │   REST clients   │   Web Dashboard       │
│   (Rust SDK)     │   (any language) │   (HTMX)              │
└────────┬─────────┴────────┬─────────┴───────────┬───────────┘
         │                  │                     │
         ▼                  ▼                     ▼
┌─────────────────────────────────────────────────────────────┐
│                    Colony Cloud API                          │
│                    (Rust/Axum)                               │
├─────────────────────────────────────────────────────────────┤
│  Auth  │  Rate Limiting  │  Usage Tracking  │  Billing      │
└────────┴────────┬────────┴─────────┬────────┴───────────────┘
                  │                  │
         ┌────────▼────────┐  ┌──────▼──────┐
         │  AWS Bedrock    │  │  PostgreSQL │
         │  (Model Proxy)  │  │  (State)    │
         └─────────────────┘  └─────────────┘
```

## Repository Structure

```
colony-cloud/
├── crates/
│   ├── colony-cloud-sdk/           # Published to crates.io
│   │   ├── src/
│   │   │   ├── lib.rs              # Public API
│   │   │   ├── client.rs           # ColonyClient
│   │   │   ├── provider.rs         # ModelProvider impl
│   │   │   ├── conversation.rs     # Local + sync memory
│   │   │   ├── models.rs           # Model types
│   │   │   └── error.rs            # SDK errors
│   │   └── Cargo.toml
│   │
│   ├── colony-cloud-api/           # Private - the server
│   │   ├── src/
│   │   │   ├── main.rs             # Axum entrypoint
│   │   │   ├── config.rs           # Environment config
│   │   │   ├── error.rs            # API error types
│   │   │   ├── middleware/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── auth.rs         # API key validation
│   │   │   │   └── rate_limit.rs   # Tier-based limiting
│   │   │   ├── routes/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── chat.rs         # POST /v1/chat/completions
│   │   │   │   ├── models.rs       # GET /v1/models
│   │   │   │   ├── conversations.rs
│   │   │   │   ├── usage.rs
│   │   │   │   ├── keys.rs
│   │   │   │   ├── subscription.rs
│   │   │   │   └── registry.rs     # Recipe registry endpoints
│   │   │   ├── bedrock/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── client.rs       # Bedrock wrapper
│   │   │   │   ├── models.rs       # Model ID mapping
│   │   │   │   ├── translate.rs    # Request/response conversion
│   │   │   │   └── stream.rs       # SSE parsing
│   │   │   ├── billing/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── stripe.rs       # Stripe integration
│   │   │   │   ├── usage.rs        # Usage tracking
│   │   │   │   └── tiers.rs        # Subscription tiers
│   │   │   ├── registry/
│   │   │   │   ├── mod.rs          # Recipe registry
│   │   │   │   ├── packages.rs     # Package management
│   │   │   │   └── versions.rs     # Version resolution
│   │   │   └── dashboard/
│   │   │       ├── mod.rs
│   │   │       ├── auth.rs         # OAuth flow
│   │   │       └── handlers.rs     # Page handlers
│   │   └── Cargo.toml
│   │
│   └── colony-cloud-common/        # Shared types
│       ├── src/
│       │   ├── lib.rs
│       │   ├── types.rs            # Shared request/response types
│       │   └── error.rs            # Shared error types
│       └── Cargo.toml
│
├── dashboard/
│   ├── templates/                  # Askama templates
│   │   ├── layout.html
│   │   ├── login.html
│   │   ├── dashboard.html
│   │   ├── usage.html
│   │   ├── keys.html
│   │   ├── subscription.html
│   │   ├── registry.html           # Recipe registry browser
│   │   └── partials/
│   │       ├── usage-chart.html
│   │       └── keys-list.html
│   └── static/
│       ├── css/
│       │   └── styles.css          # Tailwind output
│       └── js/
│           └── htmx.min.js
│
├── migrations/                     # sqlx migrations
│   ├── 001_users.sql
│   ├── 002_api_keys.sql
│   ├── 003_subscriptions.sql
│   ├── 004_usage.sql
│   ├── 005_conversations.sql
│   └── 006_registry.sql            # Recipe registry tables
│
├── tools/
│   └── bedrock-mock/               # Local development mock
│       ├── src/
│       │   └── main.rs
│       └── Cargo.toml
│
├── infra/
│   ├── environments/
│   │   ├── prod/
│   │   │   ├── main.tf
│   │   │   ├── variables.tf
│   │   │   └── terraform.tfvars
│   │   └── staging/
│   │       └── ...
│   └── modules/
│       ├── api/
│       ├── database/
│       ├── cache/
│       └── secrets/
│
├── scripts/
│   ├── dev-setup.sh
│   ├── dev-start.sh
│   ├── dev-test.sh
│   └── localstack-init.sh
│
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── deploy.yml
│
├── .claude/
│   └── system/
│       ├── features.json           # Feature tracking
│       ├── rules.txt               # Project constraints
│       └── history.txt             # Session history
│
├── Cargo.toml                      # Workspace
├── docker-compose.yml
├── Dockerfile
├── Dockerfile.dev
├── Dockerfile.bedrock-mock
├── Makefile
├── .env.example
├── CLAUDE.md                       # This file
├── RUST_STYLE_GUIDE.md
├── README.md
└── LICENSE
```

## Key Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Backend | Rust/Axum | Performance, type safety, consistency with colony-shell |
| Database | PostgreSQL + sqlx | Compile-time checked queries, production-ready |
| Cache | Redis/ElastiCache | Rate limiting, session storage |
| Auth | API keys + OAuth | CLI-friendly + web dashboard |
| Dashboard | HTMX + Askama | Server-rendered, all Rust, fast iteration |
| Infrastructure | AWS (ECS, RDS, Bedrock) | Native Bedrock integration |
| Local Dev | LocalStack + Docker Compose | Full AWS emulation locally |

## API Endpoints Summary

### Chat Completions
```
POST /v1/chat/completions    # OpenAI-compatible format
GET  /v1/models              # List available models
```

### Conversations
```
POST   /v1/conversations              # Create
GET    /v1/conversations/:id          # Fetch
DELETE /v1/conversations/:id          # Delete
POST   /v1/conversations/:id/messages # Append
```

### Usage & Billing
```
GET /v1/usage                # Current period
GET /v1/usage/history        # Historical
GET /v1/subscription         # Plan details
```

### API Keys
```
GET    /v1/keys              # List
POST   /v1/keys              # Create
DELETE /v1/keys/:id          # Revoke
```

### Recipe Registry
```
GET  /v1/registry/search     # Search packages
GET  /v1/registry/recipes    # List recipes
GET  /v1/registry/recipes/:namespace/:name
POST /v1/registry/recipes    # Publish (auth required)
```

## Subscription Tiers

| Tier | Tokens/Month | Rate Limit | Private Recipes | Price |
|------|--------------|------------|-----------------|-------|
| Free | 100K | 10 RPM | 0 | $0 |
| Pro | 5M | 100 RPM | 10 | $20/mo |
| Enterprise | 50M | 1000 RPM | Unlimited | $200/mo |

## Development Setup

### Prerequisites
- Rust 1.75+
- Docker & Docker Compose
- sqlx-cli (`cargo install sqlx-cli`)

### Quick Start
```bash
# Clone and setup
git clone git@github.com:mcfearsome/colony-cloud.git
cd colony-cloud
make setup

# Start development environment
make dev

# Run tests
make test
```

### Environment Variables
```bash
# .env (copy from .env.example)
DATABASE_URL=postgres://colony:colony@localhost:5432/colony
REDIS_URL=redis://localhost:6379
AWS_REGION=us-east-1
AWS_ENDPOINT_URL=http://localhost:4566      # LocalStack
BEDROCK_ENDPOINT=http://localhost:4567      # Mock
STRIPE_SECRET_KEY=sk_test_xxx
RUST_LOG=debug
```

---

# CLAUDE.md

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
  "notes": ["Started YYYY-MM-DD: Initial structure"]
}
```

When completing a feature:

```json
{
  "id": "F001",
  "status": "complete",
  "notes": [
    "Started YYYY-MM-DD: Initial structure",
    "Completed YYYY-MM-DD: All tests passing"
  ]
}
```

When blocked:

```json
{
  "id": "F001",
  "status": "blocked",
  "notes": ["BLOCKED: Need decision on X"]
}
```

### Updating history.txt

Log significant actions as you work:

```
[2025-01-15 14:30] START: Beginning work on F001 (API Server)
[2025-01-15 14:45] PROGRESS: Router setup complete
[2025-01-15 15:30] DECISION: Using tower middleware for auth
[2025-01-15 16:00] PROGRESS: Auth middleware working
[2025-01-15 17:00] BUG: Rate limiter not resetting properly
[2025-01-15 17:30] FIX: Fixed Redis key expiration
[2025-01-15 18:00] COMPLETE: F001 API server skeleton complete
```

### Updating rules.txt

Add rules when you discover important constraints:

```
1. All API responses must follow OpenAI format for compatibility
2. Never store raw API keys - always hash with argon2
3. Bedrock model IDs must be mapped from user-facing names
```

---

## Session End Protocol

Before ending a session:

1. **Commit work** with descriptive message referencing feature ID
2. **Update features.json** with current status and notes
3. **Add history.txt entries** for all significant work done
4. **Summarize** what was accomplished and what's next

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

---

## Rust Style Guide

Follow the Colony Shell Rust Style Guide. Key points:

### Option and Result Handling

```rust
// ❌ Avoid: Nested if-let pyramid
if let Some(a) = foo() {
    if let Some(b) = bar(a) {
        do_something(b);
    }
}

// ✅ Prefer: let-else for early returns
let Some(a) = foo() else { return None };
let Some(b) = bar(a) else { return None };
Some(do_something(b))
```

### Error Handling

```rust
// Use thiserror for library errors
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("unauthorized")]
    Unauthorized,
    
    #[error("rate limited, retry after {retry_after}s")]
    RateLimited { retry_after: u64 },
    
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

// Add context when propagating
let user = get_user(&db, user_id)
    .await
    .context("failed to fetch user for rate limit check")?;
```

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Crates | `snake_case` | `colony_cloud_api` |
| Types | `PascalCase` | `ChatRequest` |
| Functions | `snake_case` | `create_completion` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_TOKENS` |

### Async Code

```rust
// Use tokio as runtime
#[tokio::main]
async fn main() -> Result<()> {
    // ...
}

// Avoid blocking in async
async fn bad() {
    std::fs::read_to_string(path); // ❌ Blocks
}

async fn good() {
    tokio::fs::read_to_string(path).await; // ✅ Async
}
```

### Documentation

```rust
/// Create a new chat completion.
///
/// # Arguments
///
/// * `request` - The completion request with messages and options
///
/// # Returns
///
/// A streaming response of completion chunks.
///
/// # Errors
///
/// Returns `ApiError::Unauthorized` if the API key is invalid.
pub async fn create_completion(request: ChatRequest) -> Result<CompletionStream, ApiError> {
    // ...
}
```

### Clippy Lints

```toml
[lints.clippy]
all = "warn"
pedantic = "warn"
```

---

## Initial Features List

Create `.claude/system/features.json` with:

```json
{
  "features": [
    {
      "id": "F001",
      "name": "Project Scaffold",
      "description": "Set up workspace, crates, and basic structure",
      "priority": 1,
      "status": "new",
      "dependencies": [],
      "notes": []
    },
    {
      "id": "F002",
      "name": "Database Schema",
      "description": "PostgreSQL schema with sqlx migrations",
      "priority": 2,
      "status": "new",
      "dependencies": ["F001"],
      "notes": []
    },
    {
      "id": "F003",
      "name": "Auth Middleware",
      "description": "API key validation and user context",
      "priority": 3,
      "status": "new",
      "dependencies": ["F002"],
      "notes": []
    },
    {
      "id": "F004",
      "name": "Bedrock Integration",
      "description": "AWS Bedrock client with model mapping",
      "priority": 4,
      "status": "new",
      "dependencies": ["F001"],
      "notes": []
    },
    {
      "id": "F005",
      "name": "Chat Completions Endpoint",
      "description": "POST /v1/chat/completions with streaming",
      "priority": 5,
      "status": "new",
      "dependencies": ["F003", "F004"],
      "notes": []
    },
    {
      "id": "F006",
      "name": "Rate Limiting",
      "description": "Tier-based rate limiting with Redis",
      "priority": 6,
      "status": "new",
      "dependencies": ["F003"],
      "notes": []
    },
    {
      "id": "F007",
      "name": "Usage Tracking",
      "description": "Token counting and usage storage",
      "priority": 7,
      "status": "new",
      "dependencies": ["F005"],
      "notes": []
    },
    {
      "id": "F008",
      "name": "SDK - ColonyClient",
      "description": "Rust SDK with ModelProvider trait impl",
      "priority": 8,
      "status": "new",
      "dependencies": ["F005"],
      "notes": []
    },
    {
      "id": "F009",
      "name": "Stripe Integration",
      "description": "Subscription management with Stripe",
      "priority": 9,
      "status": "new",
      "dependencies": ["F007"],
      "notes": []
    },
    {
      "id": "F010",
      "name": "Dashboard - Auth",
      "description": "OAuth login with GitHub/Google",
      "priority": 10,
      "status": "new",
      "dependencies": ["F002"],
      "notes": []
    },
    {
      "id": "F011",
      "name": "Dashboard - Usage",
      "description": "Usage analytics page with HTMX",
      "priority": 11,
      "status": "new",
      "dependencies": ["F007", "F010"],
      "notes": []
    },
    {
      "id": "F012",
      "name": "Dashboard - API Keys",
      "description": "Key management page",
      "priority": 12,
      "status": "new",
      "dependencies": ["F010"],
      "notes": []
    },
    {
      "id": "F013",
      "name": "Recipe Registry - Schema",
      "description": "Database tables for recipes, plugins, skills",
      "priority": 13,
      "status": "new",
      "dependencies": ["F002"],
      "notes": []
    },
    {
      "id": "F014",
      "name": "Recipe Registry - API",
      "description": "Search, download, publish endpoints",
      "priority": 14,
      "status": "new",
      "dependencies": ["F013", "F003"],
      "notes": []
    },
    {
      "id": "F015",
      "name": "Local Development",
      "description": "Docker Compose with LocalStack and Bedrock mock",
      "priority": 15,
      "status": "new",
      "dependencies": ["F001"],
      "notes": []
    },
    {
      "id": "F016",
      "name": "Infrastructure",
      "description": "Terraform for AWS deployment",
      "priority": 16,
      "status": "new",
      "dependencies": ["F005"],
      "notes": []
    },
    {
      "id": "F017",
      "name": "CI/CD",
      "description": "GitHub Actions for test and deploy",
      "priority": 17,
      "status": "new",
      "dependencies": ["F016"],
      "notes": []
    }
  ]
}
```

---

## Initial Rules

Create `.claude/system/rules.txt` with:

```
# Colony Cloud - Project Rules

## API Design
1. All API responses must follow OpenAI format for compatibility
2. Streaming responses use Server-Sent Events (SSE)
3. Error responses include `error.type`, `error.message`, and `error.code`
4. API versioning via URL path prefix (/v1/)

## Security
5. Never store raw API keys - always hash with argon2
6. API keys use prefix "col_" for identification
7. OAuth tokens are short-lived (1 hour), refresh tokens longer (30 days)
8. All secrets from AWS Secrets Manager in production
9. Database credentials never in environment variables in production

## Bedrock Integration
10. Bedrock model IDs must be mapped from user-facing names
11. User never sees Bedrock - they see "Colony" as the provider
12. Cache Bedrock client, don't recreate per request
13. Handle Bedrock throttling with exponential backoff

## Database
14. Use sqlx with compile-time query checking
15. All timestamps are TIMESTAMPTZ (with timezone)
16. Soft delete via `deleted_at` column where appropriate
17. Use UUIDs for all primary keys

## Code Style
18. No unwrap() in production code paths
19. All public APIs need doc comments
20. Use thiserror for error types
21. Prefer let-else over nested if-let

## Testing
22. Integration tests use testcontainers for Postgres
23. Mock Bedrock responses for unit tests
24. Test both success and error paths

## Performance
25. Use connection pooling for database (sqlx default)
26. Redis for rate limiting and caching
27. Pre-allocate vectors when size is known
```

---

## Design Document Reference

For full design details, see:
- `docs/plans/2025-12-29-colony-cloud-design.md` (in colony-shell repo)
- `docs/plans/2025-12-29-recipe-system-design.md` (in colony-shell repo)

These documents contain:
- Complete API specifications
- Database schema
- SDK implementation details
- Infrastructure architecture
- Security considerations
- Local development setup
