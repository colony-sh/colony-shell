# Colony Cloud Implementation Plan

**Date**: 2025-12-29  
**Source**: [Colony Cloud Design](./2025-12-29-colony-cloud-design.md)  
**Status**: Ready for Implementation

## Executive Summary

Colony Cloud is a managed LLM service that wraps AWS Bedrock behind a REST API with tiered subscriptions. This plan breaks down the 8-week implementation timeline into atomic features with clear dependencies, acceptance criteria, and risk mitigations.

---

## Features Overview

| ID | Feature | Phase | Priority | Effort | Dependencies |
|----|---------|-------|----------|--------|--------------|
| CC-001 | Project Scaffolding | 1 | P0 | 2d | None |
| CC-002 | Database Schema & Migrations | 1 | P0 | 2d | CC-001 |
| CC-003 | Core API Server Structure | 1 | P0 | 3d | CC-001, CC-002 |
| CC-004 | API Key Authentication | 1 | P0 | 2d | CC-003 |
| CC-005 | Bedrock Client Wrapper | 1 | P0 | 3d | CC-003 |
| CC-006 | Chat Completions Endpoint | 1 | P0 | 4d | CC-004, CC-005 |
| CC-007 | Streaming Response Support | 1 | P0 | 2d | CC-006 |
| CC-008 | Models List Endpoint | 1 | P1 | 1d | CC-003 |
| CC-009 | Usage Tracking (DB Only) | 1 | P0 | 2d | CC-006 |
| CC-010 | SDK Core (ColonyClient) | 1 | P0 | 3d | CC-006, CC-007 |
| CC-011 | SDK ModelProvider Impl | 1 | P0 | 2d | CC-010 |
| CC-012 | Local Dev Environment | 1 | P0 | 3d | CC-001 |
| CC-013 | Bedrock Mock Service | 1 | P1 | 2d | CC-012 |
| CC-014 | Staging Deployment | 1 | P0 | 2d | CC-006 |
| CC-015 | Stripe Integration | 2 | P0 | 4d | CC-009 |
| CC-016 | Subscription Tiers Logic | 2 | P0 | 2d | CC-015 |
| CC-017 | Rate Limiting Middleware | 2 | P0 | 2d | CC-016 |
| CC-018 | Overage Handling | 2 | P1 | 2d | CC-016 |
| CC-019 | Usage Endpoints | 2 | P1 | 2d | CC-009 |
| CC-020 | OAuth Login (GitHub) | 3 | P0 | 3d | CC-003 |
| CC-021 | Dashboard Layout & Auth | 3 | P0 | 3d | CC-020 |
| CC-022 | Usage Analytics Page | 3 | P0 | 3d | CC-019, CC-021 |
| CC-023 | API Key Management Page | 3 | P0 | 2d | CC-021 |
| CC-024 | Subscription Management Page | 3 | P0 | 3d | CC-016, CC-021 |
| CC-025 | Additional Models (Llama, Mistral) | 4 | P1 | 2d | CC-005 |
| CC-026 | Prompt Caching Support | 4 | P1 | 2d | CC-006 |
| CC-027 | Conversation Cloud Sync | 4 | P2 | 4d | CC-006 |
| CC-028 | SDK Conversation Memory | 4 | P1 | 2d | CC-027 |
| CC-029 | Production Deployment | 4 | P0 | 3d | All above |
| CC-030 | Registry Package Tables | 4 | P1 | 2d | CC-002 |
| CC-031 | Registry API Endpoints | 4 | P1 | 3d | CC-030 |
| CC-032 | Registry Dashboard Pages | 4 | P2 | 3d | CC-031, CC-021 |

---

## Phase 1: MVP (Days 1-15)

### CC-001: Project Scaffolding

**Goal**: Create repository structure with workspace Cargo.toml, CI, and basic tooling.

**Tasks**:
1. Create `colony-cloud/` repository
2. Initialize Cargo workspace with three crates:
   - `colony-cloud-api` (binary)
   - `colony-cloud-sdk` (library, publishable)
   - `colony-cloud-common` (library)
3. Add GitHub Actions CI workflow (fmt, clippy, test)
4. Create `.env.example`, `Makefile`, `docker-compose.yml` stubs
5. Add CLAUDE.md with session protocol

**Acceptance Criteria**:
- [ ] `cargo build` succeeds
- [ ] `cargo test` runs (even if empty)
- [ ] CI workflow passes
- [ ] README.md exists with basic setup instructions

**Files to Create**:
```
colony-cloud/
├── crates/
│   ├── colony-cloud-api/
│   │   ├── src/main.rs
│   │   └── Cargo.toml
│   ├── colony-cloud-sdk/
│   │   ├── src/lib.rs
│   │   └── Cargo.toml
│   └── colony-cloud-common/
│       ├── src/lib.rs
│       └── Cargo.toml
├── .github/workflows/ci.yml
├── Cargo.toml (workspace)
├── .env.example
├── Makefile
├── docker-compose.yml
├── CLAUDE.md
└── README.md
```

**Risks**: None (straightforward)

---

### CC-002: Database Schema & Migrations

**Goal**: Create PostgreSQL schema for users, API keys, subscriptions, and usage.

**Tasks**:
1. Add `sqlx` to dependencies with offline mode support
2. Create migrations directory
3. Write migrations:
   - `001_users.sql`: users table (id, email, name, timestamps)
   - `002_api_keys.sql`: api_keys table with key_hash, prefix, user FK
   - `003_subscriptions.sql`: subscriptions with tier, limits, Stripe ID
   - `004_usage_events.sql`: per-request usage tracking
   - `005_usage_daily.sql`: daily rollup table
   - `006_conversations.sql`: conversations and messages tables
4. Add sqlx-cli to Makefile targets

**Acceptance Criteria**:
- [ ] `sqlx migrate run` succeeds against local Postgres
- [ ] `cargo sqlx prepare` generates offline data
- [ ] All tables have proper indexes
- [ ] Foreign key constraints enforced

**SQL Files**:
```sql
-- 001_users.sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    name TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);

-- 002_api_keys.sql
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_hash TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    name TEXT,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now(),
    revoked_at TIMESTAMPTZ
);
CREATE INDEX idx_api_keys_user ON api_keys(user_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_api_keys_prefix ON api_keys(key_prefix);
```

**Dependencies**: CC-001

---

### CC-003: Core API Server Structure

**Goal**: Set up Axum server with router, state, and error handling.

**Tasks**:
1. Add Axum, tokio, tracing dependencies
2. Create `config.rs` with figment for env loading
3. Create `error.rs` with ApiError enum and IntoResponse impl
4. Create `state.rs` with AppState (db pool, config)
5. Create basic router in `main.rs`
6. Add health check endpoint (`GET /health`)
7. Set up tracing-subscriber for logging

**Acceptance Criteria**:
- [ ] Server starts on configured port
- [ ] Health endpoint returns 200
- [ ] Errors return proper JSON format
- [ ] Config loads from environment

**Key Code**:
```rust
// src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("rate limited")]
    RateLimited { retry_after: u64 },
    #[error("invalid model: {0}")]
    InvalidModel(String),
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            ApiError::InvalidModel(_) => (StatusCode::BAD_REQUEST, "invalid_model"),
            ApiError::QuotaExceeded => (StatusCode::PAYMENT_REQUIRED, "quota_exceeded"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        
        let body = json!({
            "error": { "type": error_type, "message": self.to_string() }
        });
        
        (status, Json(body)).into_response()
    }
}
```

**Dependencies**: CC-001, CC-002

---

### CC-004: API Key Authentication

**Goal**: Implement middleware that validates API keys from Authorization header.

**Tasks**:
1. Create `middleware/auth.rs`
2. Implement `AuthContext` extension type
3. Add argon2 for key hashing
4. Create middleware that:
   - Extracts Bearer token
   - Queries api_keys table
   - Verifies hash (constant-time)
   - Attaches AuthContext to request
   - Updates last_used_at (async, fire-and-forget)
5. Add skip logic for public routes

**Acceptance Criteria**:
- [ ] Valid API key → request proceeds with AuthContext
- [ ] Invalid/missing key → 401 Unauthorized
- [ ] Revoked key → 401 Unauthorized
- [ ] last_used_at updates without blocking request

**Key Code**:
```rust
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Skip auth for public routes
    let path = request.uri().path();
    if path == "/health" || path.starts_with("/dashboard/login") {
        return Ok(next.run(request).await);
    }
    
    let api_key = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    
    // Lookup by prefix for efficiency, verify full hash
    let prefix = &api_key[..8.min(api_key.len())];
    let key_record = sqlx::query_as!(ApiKey,
        "SELECT * FROM api_keys WHERE key_prefix = $1 AND revoked_at IS NULL",
        prefix
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    
    // Verify full key hash
    if !argon2::verify_encoded(&key_record.key_hash, api_key.as_bytes())? {
        return Err(ApiError::Unauthorized);
    }
    
    request.extensions_mut().insert(AuthContext {
        user_id: key_record.user_id,
        api_key_id: key_record.id,
    });
    
    Ok(next.run(request).await)
}
```

**Dependencies**: CC-003

---

### CC-005: Bedrock Client Wrapper

**Goal**: Create a client that calls AWS Bedrock with proper request translation.

**Tasks**:
1. Add aws-sdk-bedrockruntime dependency
2. Create `bedrock/client.rs` with BedrockClient struct
3. Create `bedrock/models.rs` with CuratedModel definitions
4. Create `bedrock/translate.rs` for request/response conversion
5. Handle endpoint override for LocalStack/mock
6. Implement `invoke_model` (sync) and `invoke_model_with_response_stream`

**Acceptance Criteria**:
- [ ] Client connects to real Bedrock (with AWS credentials)
- [ ] Client connects to mock endpoint (with BEDROCK_ENDPOINT env var)
- [ ] Request translation works for Claude models
- [ ] Response parsing extracts content and token counts

**Model Mapping**:
```rust
pub static CURATED_MODELS: phf::Map<&str, CuratedModel> = phf_map! {
    "claude-3.5-sonnet" => CuratedModel {
        id: "claude-3.5-sonnet",
        bedrock_id: "anthropic.claude-3-5-sonnet-20241022-v2:0",
        context_window: 200_000,
        supports_tools: true,
        supports_caching: true,
    },
    "claude-3-haiku" => CuratedModel {
        id: "claude-3-haiku",
        bedrock_id: "anthropic.claude-3-haiku-20240307-v1:0",
        context_window: 200_000,
        supports_tools: true,
        supports_caching: true,
    },
};
```

**Dependencies**: CC-003

---

### CC-006: Chat Completions Endpoint

**Goal**: Implement `POST /v1/chat/completions` (OpenAI-compatible format).

**Tasks**:
1. Create `routes/chat.rs`
2. Define ChatRequest and ChatResponse types in colony-cloud-common
3. Implement handler:
   - Validate model is in curated list
   - Check user's quota (basic check, detailed in Phase 2)
   - Translate to Bedrock request
   - Call Bedrock
   - Translate response to OpenAI format
   - Return JSON response
4. Add route to router

**Acceptance Criteria**:
- [ ] Valid request → proper response with content
- [ ] Invalid model → 400 with error
- [ ] Token counts in response match Bedrock
- [ ] Response format matches OpenAI spec

**Request/Response Types**:
```rust
// colony-cloud-common/src/types.rs
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub stream: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    pub cache_system_prompt: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}
```

**Dependencies**: CC-004, CC-005

---

### CC-007: Streaming Response Support

**Goal**: Add SSE streaming for chat completions.

**Tasks**:
1. Implement Bedrock stream parsing in `bedrock/stream.rs`
2. Create UsageTrackingStream wrapper that:
   - Passes through stream events
   - Accumulates token counts
   - Records usage on stream completion
3. Return `Sse<impl Stream>` when `stream: true`
4. Add proper SSE event formatting

**Acceptance Criteria**:
- [ ] stream: true → SSE response with chunks
- [ ] Chunks formatted as `data: {...}\n\n`
- [ ] Final chunk includes usage data
- [ ] Stream ends with `data: [DONE]\n\n`

**Key Code**:
```rust
if request.stream.unwrap_or(false) {
    let stream = state.bedrock
        .invoke_model_with_response_stream(bedrock_request)
        .await?;
    
    let tracked_stream = UsageTrackingStream::new(stream, state.db.clone(), auth);
    
    Ok(Sse::new(tracked_stream)
        .keep_alive(KeepAlive::default())
        .into_response())
} else {
    // ... non-streaming path
}
```

**Dependencies**: CC-006

---

### CC-008: Models List Endpoint

**Goal**: Implement `GET /v1/models` to list available models.

**Tasks**:
1. Create `routes/models.rs`
2. Return list of curated models with metadata
3. Match OpenAI's model list format

**Acceptance Criteria**:
- [ ] Returns all curated models
- [ ] Includes id, owned_by, created timestamp
- [ ] Authenticated request required

**Dependencies**: CC-003

---

### CC-009: Usage Tracking (DB Only)

**Goal**: Record every request's token usage for billing.

**Tasks**:
1. Create `billing/usage.rs`
2. Implement `track_usage` function that inserts to usage_events
3. Call after each successful completion (streaming and non-streaming)
4. Add background job to aggregate to usage_daily (cron or pg_cron)

**Acceptance Criteria**:
- [ ] Every request creates usage_event record
- [ ] Token counts accurate for both streaming and non-streaming
- [ ] Daily aggregation runs without error

**Dependencies**: CC-006

---

### CC-010: SDK Core (ColonyClient)

**Goal**: Create the public SDK crate with basic client.

**Tasks**:
1. Implement ColonyClient struct
2. Add `from_env()` constructor using COLONY_API_KEY
3. Add `new(api_key)` constructor
4. Add `with_base_url()` builder
5. Implement `models()`, `usage()`, `subscription()` methods
6. Add reqwest as HTTP client

**Acceptance Criteria**:
- [ ] `ColonyClient::from_env()` works with COLONY_API_KEY set
- [ ] `client.models().await` returns model list
- [ ] Custom base_url works (for local dev)

**Public API**:
```rust
pub struct ColonyClient { /* private */ }

impl ColonyClient {
    pub fn from_env() -> Result<Self, Error>;
    pub fn new(api_key: impl Into<String>) -> Self;
    pub fn with_base_url(self, url: impl Into<String>) -> Self;
    
    pub async fn models(&self) -> Result<Vec<Model>, Error>;
    pub async fn usage(&self) -> Result<Usage, Error>;
    pub async fn subscription(&self) -> Result<Subscription, Error>;
}
```

**Dependencies**: CC-006, CC-007

---

### CC-011: SDK ModelProvider Impl

**Goal**: Implement `ModelProvider` trait for colony-agent integration.

**Tasks**:
1. Create ColonyProvider struct wrapping ColonyClient
2. Implement ModelProvider trait:
   - `complete()` → calls chat completions endpoint
   - `provider_name()` → "colony"
   - `model_name()` → current model
   - `max_context_tokens()` → from model config
   - `health_check()` → calls models endpoint
3. Update colony-agent's ModelRouter to detect COLONY_API_KEY first

**Acceptance Criteria**:
- [ ] ColonyProvider implements ModelProvider
- [ ] Streaming works through the trait
- [ ] colony-agent auto-detects Colony Cloud when key is set

**Dependencies**: CC-010

---

### CC-012: Local Dev Environment

**Goal**: Docker Compose setup for local development.

**Tasks**:
1. Create `docker-compose.yml` with:
   - postgres (5432)
   - redis (6379)
   - localstack (4566)
2. Create `Dockerfile.dev` for API with cargo-watch
3. Create `scripts/dev-setup.sh` for first-time setup
4. Create `scripts/localstack-init.sh` for secrets
5. Add Makefile targets: `make dev`, `make setup`, `make logs`

**Acceptance Criteria**:
- [ ] `make setup` initializes everything from scratch
- [ ] `make dev` starts all services with hot reload
- [ ] LocalStack has required secrets created
- [ ] Migrations run automatically

**Dependencies**: CC-001

---

### CC-013: Bedrock Mock Service

**Goal**: Create a mock Bedrock service for local development.

**Tasks**:
1. Create `tools/bedrock-mock/` crate
2. Implement Axum server mimicking Bedrock API
3. Mock responses for Claude, Llama, Mistral
4. Add Dockerfile.bedrock-mock
5. Add to docker-compose

**Acceptance Criteria**:
- [ ] Mock returns valid Claude-format responses
- [ ] Mock supports streaming endpoint
- [ ] API server works against mock seamlessly
- [ ] Token counts are plausible (not accurate)

**Dependencies**: CC-012

---

### CC-014: Staging Deployment

**Goal**: Deploy MVP to staging environment.

**Tasks**:
1. Create `infra/environments/staging/` Terraform
2. Provision:
   - ECS Fargate cluster
   - RDS Postgres (t4g.small)
   - ElastiCache Redis (t4g.micro)
   - ALB with HTTPS
   - Secrets Manager entries
3. Create GitHub Actions deploy workflow
4. Configure Bedrock access (IAM role)

**Acceptance Criteria**:
- [ ] `terraform apply` creates all resources
- [ ] API accessible at staging URL
- [ ] Chat completions work with real Bedrock
- [ ] Costs under $150/month

**Dependencies**: CC-006

---

## Phase 2: Billing (Days 16-25)

### CC-015: Stripe Integration

**Goal**: Integrate Stripe for payment processing.

**Tasks**:
1. Add stripe-rust dependency
2. Create `billing/stripe.rs` with StripeClient
3. Implement webhook handler for subscription events
4. Handle: subscription.created, subscription.updated, subscription.deleted
5. Sync subscription status to local DB

**Acceptance Criteria**:
- [ ] Webhooks verified with Stripe signature
- [ ] Subscription changes reflected in DB within seconds
- [ ] Failed payments handled gracefully

**Dependencies**: CC-009

---

### CC-016: Subscription Tiers Logic

**Goal**: Implement tier-based limits and checks.

**Tasks**:
1. Create `billing/tiers.rs` with tier definitions
2. Implement `get_user_tier()` and `get_tier_limits()`
3. Add quota check to chat completions handler
4. Return 402 Payment Required when quota exceeded

**Tier Definitions**:
```rust
pub enum Tier {
    Free,
    Pro,
    Enterprise,
}

impl Tier {
    pub fn monthly_tokens(&self) -> u64 {
        match self {
            Tier::Free => 100_000,
            Tier::Pro => 5_000_000,
            Tier::Enterprise => 50_000_000,
        }
    }
    
    pub fn rpm_limit(&self) -> u32 {
        match self {
            Tier::Free => 10,
            Tier::Pro => 100,
            Tier::Enterprise => 1000,
        }
    }
}
```

**Dependencies**: CC-015

---

### CC-017: Rate Limiting Middleware

**Goal**: Implement tier-based rate limiting.

**Tasks**:
1. Create `middleware/rate_limit.rs`
2. Use Redis for distributed rate limiting
3. Implement sliding window algorithm
4. Return 429 with Retry-After header when limited

**Acceptance Criteria**:
- [ ] Free tier: 10 RPM enforced
- [ ] Pro tier: 100 RPM enforced
- [ ] Enterprise tier: 1000 RPM enforced
- [ ] Rate limits work across multiple API instances

**Dependencies**: CC-016

---

### CC-018: Overage Handling

**Goal**: Allow overages for paid tiers with per-token billing.

**Tasks**:
1. Add `allow_overage` field to subscriptions
2. When quota exceeded but overage allowed, continue but flag
3. Create usage items in Stripe for overage tokens
4. Bill at $0.50 per 100K tokens

**Acceptance Criteria**:
- [ ] Pro/Enterprise can exceed quota if enabled
- [ ] Overage usage tracked separately
- [ ] Stripe invoice includes overage line items

**Dependencies**: CC-016

---

### CC-019: Usage Endpoints

**Goal**: Implement usage query endpoints.

**Tasks**:
1. Create `routes/usage.rs`
2. Implement `GET /v1/usage` (current period)
3. Implement `GET /v1/usage/history` (historical)
4. Query from usage_daily for efficiency

**Acceptance Criteria**:
- [ ] Current usage shows accurate token counts
- [ ] Historical usage supports date range filtering
- [ ] Response includes model breakdown

**Dependencies**: CC-009

---

## Phase 3: Dashboard (Days 26-35)

### CC-020: OAuth Login (GitHub)

**Goal**: Implement GitHub OAuth for dashboard access.

**Tasks**:
1. Create `dashboard/auth.rs`
2. Implement `/dashboard/login` page
3. Implement `/dashboard/oauth/callback`
4. Create session with cookie
5. Create or link user account

**Acceptance Criteria**:
- [ ] "Login with GitHub" button works
- [ ] New users get Free tier subscription
- [ ] Existing users link to GitHub
- [ ] Session persists across page loads

**Dependencies**: CC-003

---

### CC-021: Dashboard Layout & Auth

**Goal**: Set up HTMX dashboard with shared layout.

**Tasks**:
1. Add askama for templating
2. Create `dashboard/templates/layout.html`
3. Set up Tailwind CSS build
4. Add HTMX from CDN
5. Create sidebar navigation
6. Add auth middleware for dashboard routes

**Acceptance Criteria**:
- [ ] Dashboard pages share consistent layout
- [ ] Navigation between pages works
- [ ] Unauthenticated users redirected to login
- [ ] User info shown in header

**Dependencies**: CC-020

---

### CC-022: Usage Analytics Page

**Goal**: Create page showing usage statistics and charts.

**Tasks**:
1. Create `dashboard/templates/usage.html`
2. Create `/dashboard/partials/usage-chart` for HTMX updates
3. Show: tokens used, requests made, by model
4. Show: current period vs limit, trend chart
5. Add date range selector

**Acceptance Criteria**:
- [ ] Chart shows daily token usage
- [ ] Model breakdown visible
- [ ] Quota bar shows usage vs limit
- [ ] Date range filtering works

**Dependencies**: CC-019, CC-021

---

### CC-023: API Key Management Page

**Goal**: Create page for managing API keys.

**Tasks**:
1. Create `dashboard/templates/keys.html`
2. Show existing keys (prefix, name, last used)
3. Add "Create Key" form (shows full key ONCE)
4. Add "Revoke" button with confirmation
5. Use HTMX for instant updates

**Acceptance Criteria**:
- [ ] New key shown only on creation
- [ ] Keys can be named
- [ ] Revoked keys no longer work
- [ ] Last used timestamp updates

**Dependencies**: CC-021

---

### CC-024: Subscription Management Page

**Goal**: Create page for subscription management.

**Tasks**:
1. Create `dashboard/templates/subscription.html`
2. Show current tier and limits
3. Add upgrade/downgrade buttons
4. Redirect to Stripe Checkout for payment
5. Show billing history

**Acceptance Criteria**:
- [ ] Current plan clearly shown
- [ ] Upgrade flow completes successfully
- [ ] Downgrade schedules for period end
- [ ] Invoices downloadable

**Dependencies**: CC-016, CC-021

---

## Phase 4: Polish (Days 36-40)

### CC-025: Additional Models

**Goal**: Add Llama and Mistral models.

**Tasks**:
1. Add model entries to CURATED_MODELS
2. Implement Llama request translation in `bedrock/translate.rs`
3. Implement Mistral request translation
4. Test with real Bedrock calls

**Acceptance Criteria**:
- [ ] `llama-3.1-70b` works
- [ ] `mistral-large` works
- [ ] Token counts accurate for each

**Dependencies**: CC-005

---

### CC-026: Prompt Caching Support

**Goal**: Enable Anthropic's prompt caching for Claude models.

**Tasks**:
1. Add `cache_system_prompt` request field
2. Modify Anthropic request builder to add cache_control
3. Track cached tokens in usage

**Acceptance Criteria**:
- [ ] cache_system_prompt=true adds cache_control to system message
- [ ] Cached tokens shown in response
- [ ] Cost savings visible in subsequent requests

**Dependencies**: CC-006

---

### CC-027: Conversation Cloud Sync

**Goal**: Implement opt-in conversation persistence.

**Tasks**:
1. Create `routes/conversations.rs`
2. Implement CRUD endpoints
3. Add conversation_id to chat completions
4. Store messages when sync enabled

**Acceptance Criteria**:
- [ ] Conversations can be created, listed, deleted
- [ ] Messages append to conversation
- [ ] Full history retrievable
- [ ] Default is NOT synced (ephemeral)

**Dependencies**: CC-006

---

### CC-028: SDK Conversation Memory

**Goal**: Add conversation memory to SDK.

**Tasks**:
1. Create `sdk/src/conversation.rs`
2. Implement Conversation struct with local storage
3. Add `with_sync()` to enable cloud sync
4. Implement `sync()` and `load()` methods

**Acceptance Criteria**:
- [ ] Local-only conversations work offline
- [ ] Synced conversations persist to cloud
- [ ] Load retrieves full history

**Dependencies**: CC-027

---

### CC-029: Production Deployment

**Goal**: Deploy to production with full infrastructure.

**Tasks**:
1. Create `infra/environments/prod/` Terraform
2. Configure multi-AZ RDS
3. Set up CloudFront with custom domain
4. Configure monitoring and alerts
5. Set up automated backups

**Acceptance Criteria**:
- [ ] api.colony.sh resolves and works
- [ ] SSL certificate valid
- [ ] Database backed up daily
- [ ] Alerts configured for errors/latency

**Dependencies**: All above

---

### CC-030: Registry Package Tables

**Goal**: Add database tables for recipe registry.

**Tasks**:
1. Create migration `007_registry.sql`
2. Add tables: registry_packages, registry_versions, registry_tags, registry_dependencies
3. Add orgs and org_members tables
4. Add appropriate indexes

**Acceptance Criteria**:
- [ ] Tables created with proper constraints
- [ ] Indexes on namespace, type, tags
- [ ] Foreign keys enforce integrity

**Dependencies**: CC-002

---

### CC-031: Registry API Endpoints

**Goal**: Implement registry REST API.

**Tasks**:
1. Create `routes/registry.rs`
2. Implement search, list, get, download endpoints (public)
3. Implement publish, update, delete endpoints (auth required)
4. Add private namespace endpoints (Pro/Enterprise)

**Acceptance Criteria**:
- [ ] Search returns relevant results
- [ ] Download returns TOML content
- [ ] Publish validates and stores package
- [ ] Private packages only visible to org members

**Dependencies**: CC-030

---

### CC-032: Registry Dashboard Pages

**Goal**: Add registry browsing to dashboard.

**Tasks**:
1. Create `dashboard/templates/registry.html`
2. Add search and browse UI
3. Show package details page
4. Add "My Packages" management

**Acceptance Criteria**:
- [ ] Users can browse and search packages
- [ ] Package detail shows description, stats, versions
- [ ] Users can manage their published packages

**Dependencies**: CC-031, CC-021

---

## Risk Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Bedrock rate limits | High | Medium | Implement retry with backoff, queue requests |
| Stripe webhook failures | High | Low | Idempotency keys, retry queue, manual reconciliation |
| Database performance | Medium | Low | Proper indexes, connection pooling, query optimization |
| LocalStack accuracy | Low | High | Integration tests against real AWS in CI |
| Cost overruns | Medium | Medium | Budget alerts, usage caps, cost allocation tags |

---

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| API Latency (p99) | < 500ms | CloudWatch |
| Uptime | 99.9% | CloudWatch |
| Error Rate | < 0.1% | CloudWatch |
| SDK Install Time | < 30s | Manual test |
| Dashboard Load Time | < 2s | Lighthouse |

---

## Open Questions

1. **Multi-region**: Should we deploy to multiple AWS regions initially?
   - Recommendation: No, single region (us-east-1) for MVP

2. **Anthropic Direct**: Should we also support direct Anthropic API (not Bedrock)?
   - Recommendation: Yes in Phase 4, as fallback option

3. **Self-hosted**: Should enterprise customers be able to self-host?
   - Recommendation: Future consideration, not MVP scope

---

## Appendix: Feature Checklist Format

Each feature when implemented should have:

```markdown
## CC-XXX: [Feature Name]

### Status: [Not Started | In Progress | Complete]

### Implementation Notes
- [Any deviations from plan]
- [Technical decisions made]

### Test Coverage
- [ ] Unit tests
- [ ] Integration tests
- [ ] E2E tests (if applicable)

### Review Checklist
- [ ] Code reviewed
- [ ] Documentation updated
- [ ] Deployed to staging
- [ ] Manually verified
```
