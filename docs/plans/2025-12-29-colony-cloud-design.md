# Colony Cloud Design

**Date**: 2025-12-29  
**Status**: Draft  
**Author**: Brainstorming session

## Overview

Colony Cloud is a managed LLM service that provides:
- REST API compatible with OpenAI's format (easy adoption)
- Native Rust SDK for colony-shell integration
- Curated models from AWS Bedrock (Claude, Llama, Mistral)
- Tiered subscriptions with included token allocations
- Privacy-first conversation memory (local default, opt-in sync)

Bedrock is the invisible backend - users interact with "Colony's models" via subscription. Users see real model names (claude-3.5-sonnet, llama-3.1-70b) but only models vetted and priced by Colony.

## Architecture

### High-Level Diagram

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

### Repository Structure

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
│   │   │   │   └── subscription.rs
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
│   ├── templates/
│   │   ├── layout.html
│   │   ├── login.html
│   │   ├── dashboard.html
│   │   ├── usage.html
│   │   ├── keys.html
│   │   ├── subscription.html
│   │   └── partials/
│   │       ├── usage-chart.html
│   │       └── keys-list.html
│   └── static/
│       ├── css/
│       │   └── styles.css          # Tailwind output
│       └── js/
│           └── htmx.min.js
│
├── migrations/
│   ├── 001_users.sql
│   ├── 002_api_keys.sql
│   ├── 003_subscriptions.sql
│   ├── 004_usage.sql
│   └── 005_conversations.sql
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
├── tools/
│   └── bedrock-mock/               # Local development mock
│       ├── src/
│       │   └── main.rs
│       └── Cargo.toml
│
├── scripts/
│   ├── dev-setup.sh
│   ├── dev-start.sh
│   ├── dev-test.sh
│   └── localstack-init.sh
│
├── .github/
│   └── workflows/
│       ├── ci.yml                  # Test on PR
│       └── deploy.yml              # Deploy on main
│
├── Cargo.toml                      # Workspace
├── docker-compose.yml
├── Dockerfile
├── Dockerfile.dev
├── Dockerfile.bedrock-mock
├── Makefile
├── .env.example
├── README.md
└── LICENSE
```

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Architecture | REST API + Rust SDK | Polyglot support + native colony-shell integration |
| Auth | API keys + OAuth | CLI-friendly + web dashboard |
| Models | Pass-through curated | Transparent, vetted, priced by Colony |
| Pricing | Tiered subscriptions | Predictable revenue, user-friendly |
| Memory | Local-first, opt-in sync | Privacy-respecting |
| Provider priority | Colony Cloud primary | Seamless for subscribers |
| Backend | Rust/Axum | Consistent with colony-shell |
| Repo structure | Monorepo, SDK published | Coordination + independent consumption |
| Dashboard | HTMX + templates | All Rust, fast iteration |
| Infrastructure | AWS (ECS, RDS, Bedrock) | Native Bedrock integration |

## API Design

### Base URL & Versioning

```
https://api.colony.sh/v1/
```

### Authentication

All API requests require `Authorization: Bearer <COLONY_API_KEY>` header.

### Core Endpoints

**Chat Completions** (OpenAI-compatible format):
```
POST /v1/chat/completions
```

```json
{
  "model": "claude-3.5-sonnet",
  "messages": [
    {"role": "system", "content": "You are helpful."},
    {"role": "user", "content": "Hello!"}
  ],
  "stream": true,
  "max_tokens": 4096,
  "tools": [],
  "cache_system_prompt": true
}
```

**Available Models**:
```
GET /v1/models
```

**Conversations** (opt-in persistence):
```
POST   /v1/conversations              # Create with cloud sync enabled
GET    /v1/conversations/:id          # Fetch history
DELETE /v1/conversations/:id          # Delete
POST   /v1/conversations/:id/messages # Append to existing
```

**Usage & Billing**:
```
GET /v1/usage                    # Current period usage
GET /v1/usage/history            # Historical usage
GET /v1/subscription             # Current plan, limits, overage
```

**API Key Management**:
```
GET    /v1/keys                  # List keys
POST   /v1/keys                  # Create key
DELETE /v1/keys/:id              # Revoke key
```

### Response Format

Follows OpenAI's format for compatibility:
```json
{
  "id": "msg_abc123",
  "object": "chat.completion",
  "created": 1704067200,
  "model": "claude-3.5-sonnet",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "Hello!"},
    "finish_reason": "stop"
  }],
  "usage": {
    "prompt_tokens": 12,
    "completion_tokens": 8,
    "total_tokens": 20
  }
}
```

## SDK Design

The Rust SDK is a thin client that calls the API. It implements the `ModelProvider` trait from colony-agent for seamless integration.

### Public API

```rust
pub struct ColonyClient {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl ColonyClient {
    /// Create client from COLONY_API_KEY env var
    pub fn from_env() -> Result<Self>;
    
    /// Create with explicit key
    pub fn new(api_key: impl Into<String>) -> Self;
    
    /// Override base URL (for self-hosted/testing)
    pub fn with_base_url(self, url: impl Into<String>) -> Self;
    
    /// List available models
    pub async fn models(&self) -> Result<Vec<Model>>;
    
    /// Get usage for current billing period
    pub async fn usage(&self) -> Result<Usage>;
    
    /// Get subscription info
    pub async fn subscription(&self) -> Result<Subscription>;
}
```

### ModelProvider Implementation

```rust
pub struct ColonyProvider {
    client: ColonyClient,
    model: String,
}

impl ColonyProvider {
    pub fn new(client: ColonyClient, model: &str) -> Self;
}

#[async_trait]
impl ModelProvider for ColonyProvider {
    async fn complete(&mut self, request: CompletionRequest) -> Result<CompletionStream>;
    fn provider_name(&self) -> &str { "colony" }
    fn model_name(&self) -> &str { &self.model }
    fn max_context_tokens(&self) -> usize { /* per model */ }
    async fn health_check(&self) -> Result<ProviderHealth>;
}
```

### Integration with ModelRouter

```rust
// Updated detection order in colony-agent:
// 1. COLONY_API_KEY → ColonyProvider (primary)
// 2. ANTHROPIC_API_KEY → AnthropicProvider
// 3. OPENAI_API_KEY → OpenAIProvider  
// 4. Ollama → OllamaProvider (free fallback)

impl ModelRouter {
    pub async fn auto_detect() -> Result<Self> {
        // Colony Cloud takes priority
        if let Ok(api_key) = env::var("COLONY_API_KEY") {
            let client = ColonyClient::new(api_key);
            let model = env::var("COLONY_MODEL")
                .unwrap_or_else(|_| "claude-3.5-sonnet".into());
            return Ok(Self::with_provider(
                Box::new(ColonyProvider::new(client, &model))
            ));
        }
        // ... existing fallback chain
    }
}
```

### Conversation Memory (Local + Opt-in Sync)

```rust
pub struct Conversation {
    id: ConversationId,
    messages: Vec<Message>,
    sync_enabled: bool,
}

impl Conversation {
    /// Local-only conversation (default)
    pub fn new() -> Self;
    
    /// Enable cloud sync for this conversation
    pub fn with_sync(self, client: &ColonyClient) -> Self;
    
    /// Sync local state to cloud
    pub async fn sync(&self, client: &ColonyClient) -> Result<()>;
    
    /// Load from cloud
    pub async fn load(client: &ColonyClient, id: ConversationId) -> Result<Self>;
}
```

## Database Schema

PostgreSQL for all persistent state. Using `sqlx` for compile-time checked queries.

### Core Tables

```sql
-- Users (OAuth-authenticated)
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    name TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);

-- API Keys (one user can have many)
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    key_hash TEXT NOT NULL,           -- argon2 hash of key
    key_prefix TEXT NOT NULL,         -- "col_xxxx" for display
    name TEXT,                        -- user-friendly label
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now(),
    revoked_at TIMESTAMPTZ            -- soft delete
);

-- Subscriptions
CREATE TABLE subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    tier TEXT NOT NULL,               -- 'free', 'pro', 'enterprise'
    status TEXT NOT NULL,             -- 'active', 'cancelled', 'past_due'
    tokens_included BIGINT NOT NULL,  -- monthly allocation
    current_period_start TIMESTAMPTZ NOT NULL,
    current_period_end TIMESTAMPTZ NOT NULL,
    stripe_subscription_id TEXT,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Usage tracking (per request)
CREATE TABLE usage_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),
    api_key_id UUID REFERENCES api_keys(id),
    model TEXT NOT NULL,
    input_tokens INT NOT NULL,
    output_tokens INT NOT NULL,
    cached_tokens INT DEFAULT 0,
    latency_ms INT,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Usage rollups (daily aggregates for fast queries)
CREATE TABLE usage_daily (
    user_id UUID REFERENCES users(id),
    date DATE NOT NULL,
    model TEXT NOT NULL,
    input_tokens BIGINT DEFAULT 0,
    output_tokens BIGINT DEFAULT 0,
    cached_tokens BIGINT DEFAULT 0,
    request_count INT DEFAULT 0,
    PRIMARY KEY (user_id, date, model)
);

-- Conversations (opt-in cloud sync only)
CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    title TEXT,
    model TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,               -- 'user', 'assistant', 'system'
    content JSONB NOT NULL,           -- supports text + tool calls
    tokens INT,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- Indexes
CREATE INDEX idx_api_keys_user ON api_keys(user_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_usage_events_user_created ON usage_events(user_id, created_at);
CREATE INDEX idx_usage_daily_user_date ON usage_daily(user_id, date);
CREATE INDEX idx_conversations_user ON conversations(user_id);
CREATE INDEX idx_messages_conversation ON messages(conversation_id);
```

### Subscription Tiers

| Tier | Tokens/Month | Rate Limit | Price |
|------|--------------|------------|-------|
| Free | 100K | 10 RPM | $0 |
| Pro | 5M | 100 RPM | $20/mo |
| Enterprise | 50M | 1000 RPM | $200/mo |

Overage: $0.50 per 100K tokens (competitive with direct Bedrock pricing + margin).

## API Server Implementation

### Application Structure

```rust
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();
    
    let config = Config::from_env()?;
    let db = PgPool::connect(&config.database_url).await?;
    let bedrock = BedrockClient::new(&config).await?;
    let stripe = StripeClient::new(&config.stripe_secret_key);
    
    let state = AppState { db, bedrock, stripe, config };
    
    let app = Router::new()
        // Chat completions (the core endpoint)
        .route("/v1/chat/completions", post(chat_completions))
        // Models
        .route("/v1/models", get(list_models))
        // Conversations
        .route("/v1/conversations", post(create_conversation))
        .route("/v1/conversations/:id", get(get_conversation).delete(delete_conversation))
        .route("/v1/conversations/:id/messages", post(append_message))
        // Usage & billing
        .route("/v1/usage", get(get_usage))
        .route("/v1/usage/history", get(get_usage_history))
        .route("/v1/subscription", get(get_subscription))
        // API keys
        .route("/v1/keys", get(list_keys).post(create_key))
        .route("/v1/keys/:id", delete(revoke_key))
        // Dashboard (HTMX)
        .nest("/dashboard", dashboard_routes())
        // Middleware
        .layer(from_fn_with_state(state.clone(), auth_middleware))
        .layer(from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    axum::serve(TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
```

### Auth Middleware

```rust
async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Skip auth for public routes
    if request.uri().path().starts_with("/dashboard/login") {
        return Ok(next.run(request).await);
    }
    
    let api_key = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    
    // Lookup key, verify not revoked
    let key_record = sqlx::query_as!(ApiKey,
        "SELECT * FROM api_keys WHERE key_hash = crypt($1, key_hash) AND revoked_at IS NULL",
        api_key
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    
    // Attach user context to request
    request.extensions_mut().insert(AuthContext {
        user_id: key_record.user_id,
        api_key_id: key_record.id,
    });
    
    // Update last_used_at (fire and forget)
    let db = state.db.clone();
    let key_id = key_record.id;
    tokio::spawn(async move {
        let _ = sqlx::query!("UPDATE api_keys SET last_used_at = now() WHERE id = $1", key_id)
            .execute(&db).await;
    });
    
    Ok(next.run(request).await)
}
```

### Rate Limiting Middleware

```rust
async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let auth = request.extensions().get::<AuthContext>()
        .ok_or(ApiError::Unauthorized)?;
    
    // Get user's tier and limits
    let sub = sqlx::query_as!(Subscription,
        "SELECT * FROM subscriptions WHERE user_id = $1 AND status = 'active'",
        auth.user_id
    )
    .fetch_optional(&state.db)
    .await?;
    
    let rpm_limit = match sub.map(|s| s.tier.as_str()) {
        Some("enterprise") => 1000,
        Some("pro") => 100,
        _ => 10, // free
    };
    
    // Check rate limit (Redis or in-memory with dashmap)
    let key = format!("rate:{}:{}", auth.user_id, current_minute());
    let count = state.rate_limiter.increment(&key).await;
    
    if count > rpm_limit {
        return Err(ApiError::RateLimited { retry_after: 60 });
    }
    
    Ok(next.run(request).await)
}
```

### Chat Completions Handler

```rust
async fn chat_completions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<ChatRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate model is available
    let model = CURATED_MODELS.get(&request.model)
        .ok_or(ApiError::InvalidModel)?;
    
    // Check token budget
    let usage = get_current_period_usage(&state.db, auth.user_id).await?;
    let sub = get_subscription(&state.db, auth.user_id).await?;
    
    if usage.total_tokens >= sub.tokens_included {
        // Check if overage is allowed or return error
        if !sub.allow_overage {
            return Err(ApiError::QuotaExceeded);
        }
    }
    
    // Build Bedrock request
    let bedrock_request = build_bedrock_request(&request, model)?;
    
    if request.stream.unwrap_or(false) {
        // Streaming response
        let stream = state.bedrock
            .invoke_model_with_response_stream(bedrock_request)
            .await?;
        
        let tracked_stream = UsageTrackingStream::new(
            stream,
            state.db.clone(),
            auth,
            request.model.clone(),
        );
        
        Ok(Sse::new(tracked_stream).into_response())
    } else {
        // Non-streaming
        let response = state.bedrock.invoke_model(bedrock_request).await?;
        
        // Track usage
        track_usage(&state.db, &auth, &request.model, &response).await?;
        
        Ok(Json(convert_to_openai_format(response)).into_response())
    }
}
```

## Bedrock Integration

### Client Wrapper

```rust
pub struct BedrockClient {
    client: aws_sdk_bedrockruntime::Client,
    region: String,
    is_mock: bool,
}

impl BedrockClient {
    pub async fn new(config: &Config) -> Result<Self> {
        let mut aws_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.aws_region.clone()));
        
        // Use LocalStack endpoint if configured
        if let Some(endpoint) = &config.aws_endpoint_url {
            aws_config = aws_config.endpoint_url(endpoint);
        }
        
        let aws_config = aws_config.load().await;
        
        // Use mock Bedrock endpoint for local dev
        let bedrock_endpoint = config.bedrock_endpoint.clone()
            .unwrap_or_else(|| "https://bedrock-runtime.us-east-1.amazonaws.com".into());
        
        let bedrock_config = aws_sdk_bedrockruntime::config::Builder::from(&aws_config)
            .endpoint_url(&bedrock_endpoint)
            .build();
        
        let client = aws_sdk_bedrockruntime::Client::from_conf(bedrock_config);
        
        Ok(Self { 
            client, 
            region: config.aws_region.clone(),
            is_mock: config.bedrock_endpoint.is_some(),
        })
    }
    
    pub async fn invoke_model(&self, request: BedrockRequest) -> Result<BedrockResponse> {
        let model_id = &request.model_id;
        let body = serde_json::to_vec(&request.body)?;
        
        let response = self.client
            .invoke_model()
            .model_id(model_id)
            .content_type("application/json")
            .body(Blob::new(body))
            .send()
            .await?;
        
        let response_body: serde_json::Value = 
            serde_json::from_slice(response.body().as_ref())?;
        
        Ok(BedrockResponse {
            content: response_body,
            input_tokens: extract_input_tokens(&response_body),
            output_tokens: extract_output_tokens(&response_body),
        })
    }
    
    pub async fn invoke_model_with_response_stream(
        &self,
        request: BedrockRequest,
    ) -> Result<impl Stream<Item = Result<StreamChunk>>> {
        let model_id = &request.model_id;
        let body = serde_json::to_vec(&request.body)?;
        
        let response = self.client
            .invoke_model_with_response_stream()
            .model_id(model_id)
            .content_type("application/json")
            .body(Blob::new(body))
            .send()
            .await?;
        
        Ok(BedrockStreamParser::new(response.body))
    }
}
```

### Model ID Mapping

```rust
pub struct CuratedModel {
    pub id: &'static str,              // User-facing: "claude-3.5-sonnet"
    pub bedrock_id: &'static str,      // Bedrock: "anthropic.claude-3-5-sonnet-20241022-v2:0"
    pub context_window: usize,
    pub input_cost_per_1m: f64,        // Our pricing (includes margin)
    pub output_cost_per_1m: f64,
    pub supports_tools: bool,
    pub supports_caching: bool,
}

pub static CURATED_MODELS: phf::Map<&'static str, CuratedModel> = phf_map! {
    "claude-3.5-sonnet" => CuratedModel {
        id: "claude-3.5-sonnet",
        bedrock_id: "anthropic.claude-3-5-sonnet-20241022-v2:0",
        context_window: 200_000,
        input_cost_per_1m: 3.50,   // Bedrock: $3.00, margin: ~17%
        output_cost_per_1m: 17.50, // Bedrock: $15.00
        supports_tools: true,
        supports_caching: true,
    },
    "claude-3-opus" => CuratedModel {
        id: "claude-3-opus",
        bedrock_id: "anthropic.claude-3-opus-20240229-v1:0",
        context_window: 200_000,
        input_cost_per_1m: 17.50,
        output_cost_per_1m: 87.50,
        supports_tools: true,
        supports_caching: true,
    },
    "claude-3-haiku" => CuratedModel {
        id: "claude-3-haiku",
        bedrock_id: "anthropic.claude-3-haiku-20240307-v1:0",
        context_window: 200_000,
        input_cost_per_1m: 0.30,
        output_cost_per_1m: 1.50,
        supports_tools: true,
        supports_caching: true,
    },
    "llama-3.1-70b" => CuratedModel {
        id: "llama-3.1-70b",
        bedrock_id: "meta.llama3-1-70b-instruct-v1:0",
        context_window: 128_000,
        input_cost_per_1m: 1.10,
        output_cost_per_1m: 1.10,
        supports_tools: false,
        supports_caching: false,
    },
    "mistral-large" => CuratedModel {
        id: "mistral-large",
        bedrock_id: "mistral.mistral-large-2407-v1:0",
        context_window: 128_000,
        input_cost_per_1m: 4.50,
        output_cost_per_1m: 13.50,
        supports_tools: true,
        supports_caching: false,
    },
};
```

### Request Translation

```rust
pub fn build_bedrock_request(
    request: &ChatRequest,
    model: &CuratedModel,
) -> Result<BedrockRequest> {
    // Claude models use Anthropic's Messages API format
    if model.bedrock_id.starts_with("anthropic.") {
        return build_anthropic_request(request, model);
    }
    
    // Llama/Mistral use different formats
    if model.bedrock_id.starts_with("meta.") {
        return build_llama_request(request, model);
    }
    
    if model.bedrock_id.starts_with("mistral.") {
        return build_mistral_request(request, model);
    }
    
    Err(ApiError::UnsupportedModel)
}

fn build_anthropic_request(
    request: &ChatRequest,
    model: &CuratedModel,
) -> Result<BedrockRequest> {
    let (system, messages) = extract_system_and_messages(&request.messages);
    
    let mut body = json!({
        "anthropic_version": "bedrock-2023-05-31",
        "max_tokens": request.max_tokens.unwrap_or(4096),
        "messages": messages,
    });
    
    if let Some(system) = system {
        if request.cache_system_prompt.unwrap_or(false) && model.supports_caching {
            body["system"] = json!([{
                "type": "text",
                "text": system,
                "cache_control": {"type": "ephemeral"}
            }]);
        } else {
            body["system"] = json!(system);
        }
    }
    
    if !request.tools.is_empty() {
        body["tools"] = json!(request.tools);
    }
    
    if let Some(temp) = request.temperature {
        body["temperature"] = json!(temp);
    }
    
    Ok(BedrockRequest {
        model_id: model.bedrock_id.to_string(),
        body,
    })
}
```

## Dashboard (HTMX)

Server-rendered dashboard for subscription management, usage analytics, and API key management.

### Route Structure

```rust
pub fn dashboard_routes() -> Router<AppState> {
    Router::new()
        // Auth
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", post(logout))
        .route("/oauth/callback", get(oauth_callback))
        // Main pages
        .route("/", get(dashboard_home))
        .route("/usage", get(usage_page))
        .route("/keys", get(keys_page))
        .route("/subscription", get(subscription_page))
        // HTMX partials (return HTML fragments)
        .route("/partials/usage-chart", get(usage_chart_partial))
        .route("/partials/keys-list", get(keys_list_partial))
        // Actions
        .route("/keys/create", post(create_key_action))
        .route("/keys/:id/revoke", post(revoke_key_action))
        // Static assets
        .nest_service("/static", ServeDir::new("dashboard/static"))
}
```

### Templates with Askama

```rust
use askama::Template;

#[derive(Template)]
#[template(path = "layout.html")]
pub struct LayoutTemplate<'a> {
    pub title: &'a str,
    pub user: &'a User,
    pub content: String,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate<'a> {
    pub user: &'a User,
    pub usage: &'a UsageSummary,
    pub subscription: &'a Subscription,
}

#[derive(Template)]
#[template(path = "keys.html")]
pub struct KeysTemplate<'a> {
    pub user: &'a User,
    pub keys: &'a [ApiKeyDisplay],
}
```

## Infrastructure & Deployment

### AWS Architecture

```
                    ┌─────────────────┐
                    │   Route 53      │
                    │  api.colony.sh  │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  CloudFront     │
                    │  (TLS, caching) │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  ALB            │
                    │  (health checks)│
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
      ┌───────▼───────┐ ┌───▼───┐ ┌───────▼───────┐
      │  ECS Fargate  │ │  ...  │ │  ECS Fargate  │
      │  (API server) │ │       │ │  (API server) │
      └───────┬───────┘ └───────┘ └───────┬───────┘
              │                           │
              └─────────────┬─────────────┘
                            │
         ┌──────────────────┼──────────────────┐
         │                  │                  │
 ┌───────▼───────┐  ┌───────▼───────┐  ┌──────▼──────┐
 │  RDS Postgres │  │  ElastiCache  │  │  Bedrock    │
 │  (state)      │  │  (rate limit) │  │  (models)   │
 └───────────────┘  └───────────────┘  └─────────────┘
```

### Terraform Structure

```
infra/
├── environments/
│   ├── prod/
│   │   ├── main.tf
│   │   ├── variables.tf
│   │   └── terraform.tfvars
│   └── staging/
│       └── ...
├── modules/
│   ├── api/           # ECS service, ALB, CloudFront
│   ├── database/      # RDS Postgres
│   ├── cache/         # ElastiCache Redis
│   └── secrets/       # Secrets Manager
└── main.tf
```

### Estimated Monthly Costs (Early Stage)

| Service | Config | Cost |
|---------|--------|------|
| ECS Fargate | 2x 0.5vCPU/1GB | ~$30 |
| RDS Postgres | db.t4g.small | ~$25 |
| ElastiCache | cache.t4g.micro | ~$12 |
| ALB | 1 load balancer | ~$20 |
| CloudFront | Light traffic | ~$5 |
| NAT Gateway | 1 (single AZ) | ~$35 |
| **Total** | | **~$127/mo** |

Bedrock costs are pass-through on your AWS bill.

### CI/CD (GitHub Actions)

```yaml
name: Deploy

on:
  push:
    branches: [main]

env:
  AWS_REGION: us-east-1
  ECR_REPOSITORY: colony-cloud-api

jobs:
  build-and-deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Configure AWS credentials
        uses: aws-actions/configure-aws-credentials@v4
        with:
          aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
          aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
          aws-region: ${{ env.AWS_REGION }}
      
      - name: Login to ECR
        id: login-ecr
        uses: aws-actions/amazon-ecr-login@v2
      
      - name: Build and push
        env:
          ECR_REGISTRY: ${{ steps.login-ecr.outputs.registry }}
          IMAGE_TAG: ${{ github.sha }}
        run: |
          docker build -t $ECR_REGISTRY/$ECR_REPOSITORY:$IMAGE_TAG .
          docker push $ECR_REGISTRY/$ECR_REPOSITORY:$IMAGE_TAG
      
      - name: Deploy to ECS
        run: |
          aws ecs update-service \
            --cluster colony-cloud \
            --service api \
            --force-new-deployment
```

## Security & Compliance

### API Key Security

```rust
// Key generation - cryptographically secure
pub fn generate_api_key() -> (String, String) {
    let raw_key = format!("col_{}", generate_secure_random(32));
    let prefix = raw_key[..8].to_string();  // "col_xxxx" for display
    let hash = argon2::hash_encoded(
        raw_key.as_bytes(),
        &generate_salt(),
        &argon2::Config::default()
    ).unwrap();
    
    (raw_key, hash)  // Return raw to user ONCE, store hash
}

// Key verification - constant-time comparison
pub fn verify_api_key(raw_key: &str, stored_hash: &str) -> bool {
    argon2::verify_encoded(stored_hash, raw_key.as_bytes())
        .unwrap_or(false)
}
```

### Secrets Management

All secrets from AWS Secrets Manager, never in code/env files:

```rust
pub struct Secrets {
    pub database_url: String,
    pub redis_url: String,
    pub stripe_secret_key: String,
    pub oauth_client_secret: String,
}

impl Secrets {
    pub async fn load() -> Result<Self> {
        let client = aws_sdk_secretsmanager::Client::new(&aws_config::load_from_env().await);
        
        Ok(Self {
            database_url: get_secret(&client, "colony/prod/database_url").await?,
            redis_url: get_secret(&client, "colony/prod/redis_url").await?,
            stripe_secret_key: get_secret(&client, "colony/prod/stripe").await?,
            oauth_client_secret: get_secret(&client, "colony/prod/oauth").await?,
        })
    }
}
```

### Data Privacy

```rust
// Conversation storage - encrypted at rest (RDS default)
// User controls what persists

pub enum ConversationRetention {
    Ephemeral,           // Not stored - default
    Session,             // Stored until logout
    Persistent,          // Stored until deleted
}

// No conversation content in logs
impl Message {
    pub fn redacted_for_logging(&self) -> String {
        format!("[{} message, {} chars]", self.role, self.content.len())
    }
}

// GDPR: User data export
pub async fn export_user_data(db: &PgPool, user_id: Uuid) -> Result<UserDataExport>;

// GDPR: User data deletion
pub async fn delete_user_data(db: &PgPool, user_id: Uuid) -> Result<()>;
```

## Local Development with LocalStack

LocalStack emulates AWS services locally, enabling full development without AWS costs or credentials.

### Services Used

| AWS Service | LocalStack Support | Usage |
|-------------|-------------------|-------|
| Secrets Manager | ✅ Full | Store local dev secrets |
| S3 | ✅ Full | Static assets (future) |
| CloudWatch Logs | ✅ Full | Log aggregation |
| SQS | ✅ Full | Async job queue (future) |
| Bedrock | ⚠️ Mock only | Mock responses for testing |

**Note**: LocalStack doesn't run actual Bedrock models. We create a mock Bedrock service for local dev.

### Docker Compose Setup

```yaml
# docker-compose.yml
services:
  api:
    build:
      context: .
      dockerfile: Dockerfile.dev
    ports:
      - "8080:8080"
    environment:
      - RUST_LOG=debug
      - DATABASE_URL=postgres://colony:colony@postgres:5432/colony
      - REDIS_URL=redis://redis:6379
      - AWS_ENDPOINT_URL=http://localstack:4566
      - AWS_ACCESS_KEY_ID=test
      - AWS_SECRET_ACCESS_KEY=test
      - AWS_REGION=us-east-1
      - BEDROCK_ENDPOINT=http://bedrock-mock:4567
      - STRIPE_SECRET_KEY=sk_test_xxx
    depends_on:
      - postgres
      - redis
      - localstack
      - bedrock-mock
    volumes:
      - ./crates:/app/crates
      - cargo-cache:/usr/local/cargo/registry

  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: colony
      POSTGRES_PASSWORD: colony
      POSTGRES_DB: colony
    ports:
      - "5432:5432"
    volumes:
      - postgres-data:/var/lib/postgresql/data

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"

  localstack:
    image: localstack/localstack:latest
    ports:
      - "4566:4566"
      - "4510-4559:4510-4559"
    environment:
      - SERVICES=secretsmanager,s3,logs
      - DEBUG=1
      - PERSISTENCE=1
    volumes:
      - localstack-data:/var/lib/localstack
      - ./scripts/localstack-init.sh:/etc/localstack/init/ready.d/init.sh

  bedrock-mock:
    build:
      context: .
      dockerfile: Dockerfile.bedrock-mock
    ports:
      - "4567:4567"
    environment:
      - RUST_LOG=debug

volumes:
  postgres-data:
  localstack-data:
  cargo-cache:
```

### LocalStack Initialization Script

```bash
#!/bin/bash
# scripts/localstack-init.sh

# Create secrets
awslocal secretsmanager create-secret \
  --name colony/local/database_url \
  --secret-string "postgres://colony:colony@postgres:5432/colony"

awslocal secretsmanager create-secret \
  --name colony/local/redis_url \
  --secret-string "redis://redis:6379"

awslocal secretsmanager create-secret \
  --name colony/local/stripe \
  --secret-string "sk_test_local_development_key"

awslocal secretsmanager create-secret \
  --name colony/local/oauth \
  --secret-string "local_oauth_secret"

# Create CloudWatch log group
awslocal logs create-log-group --log-group-name /colony/api

echo "LocalStack initialized!"
```

### Bedrock Mock Service

A lightweight Axum service that mimics Bedrock's API:

```rust
// tools/bedrock-mock/src/main.rs

use axum::{routing::post, Json, Router};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/model/:model_id/invoke", post(invoke_model))
        .route("/model/:model_id/invoke-with-response-stream", post(invoke_stream));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:4567").await.unwrap();
    println!("Bedrock mock running on http://localhost:4567");
    axum::serve(listener, app).await.unwrap();
}

async fn invoke_model(
    axum::extract::Path(model_id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    // Simulate latency
    sleep(Duration::from_millis(100)).await;
    
    // Return mock response based on model
    if model_id.contains("claude") {
        Json(mock_claude_response(&body))
    } else if model_id.contains("llama") {
        Json(mock_llama_response(&body))
    } else {
        Json(mock_generic_response(&body))
    }
}

fn mock_claude_response(request: &serde_json::Value) -> serde_json::Value {
    let user_message = request["messages"]
        .as_array()
        .and_then(|m| m.last())
        .and_then(|m| m["content"].as_str())
        .unwrap_or("Hello");
    
    serde_json::json!({
        "id": "msg_mock_123",
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "text",
            "text": format!("[MOCK] Response to: {}", &user_message[..50.min(user_message.len())])
        }],
        "model": "claude-3-5-sonnet-20241022",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50
        }
    })
}
```

### Developer Scripts

```bash
# scripts/dev-setup.sh
#!/bin/bash
set -e

echo "Setting up local development environment..."

# Install dependencies
if ! command -v docker &> /dev/null; then
    echo "Docker is required. Install from https://docker.com"
    exit 1
fi

if ! command -v sqlx &> /dev/null; then
    echo "Installing sqlx-cli..."
    cargo install sqlx-cli --no-default-features --features postgres
fi

# Start services
docker compose up -d postgres redis localstack bedrock-mock

# Wait for postgres
echo "Waiting for Postgres..."
until docker compose exec -T postgres pg_isready; do sleep 1; done

# Run migrations
echo "Running migrations..."
DATABASE_URL="postgres://colony:colony@localhost:5432/colony" sqlx migrate run

echo "Ready! Run 'cargo run -p colony-cloud-api' to start the server."
```

### Makefile

```makefile
.PHONY: dev setup test clean logs

# Start full local environment
dev:
	docker compose up -d
	cargo watch -x 'run -p colony-cloud-api'

# First-time setup
setup:
	./scripts/dev-setup.sh

# Run tests
test:
	./scripts/dev-test.sh

# View logs
logs:
	docker compose logs -f

# Clean up
clean:
	docker compose down -v
	cargo clean

# Run migrations
migrate:
	DATABASE_URL="postgres://colony:colony@localhost:5432/colony" sqlx migrate run

# Generate sqlx offline data
sqlx-prepare:
	DATABASE_URL="postgres://colony:colony@localhost:5432/colony" cargo sqlx prepare

# Format and lint
lint:
	cargo fmt --check
	cargo clippy -- -D warnings
```

### Environment Variables

```bash
# .env.example - Copy to .env for local development

# Database
DATABASE_URL=postgres://colony:colony@localhost:5432/colony

# Redis
REDIS_URL=redis://localhost:6379

# AWS / LocalStack
AWS_REGION=us-east-1
AWS_ACCESS_KEY_ID=test
AWS_SECRET_ACCESS_KEY=test
AWS_ENDPOINT_URL=http://localhost:4566

# Bedrock Mock
BEDROCK_ENDPOINT=http://localhost:4567

# Stripe (test mode)
STRIPE_SECRET_KEY=sk_test_xxx

# App
RUST_LOG=debug
ENVIRONMENT=local
PORT=8080
```

## Implementation Phases

### Phase 1: MVP (Weeks 1-3)
- [ ] Basic API server with chat completions
- [ ] Bedrock integration (Claude models only)
- [ ] API key auth (no dashboard yet)
- [ ] Usage tracking (DB only, no dashboard)
- [ ] SDK with ModelProvider impl
- [ ] Deploy to staging

### Phase 2: Billing (Weeks 4-5)
- [ ] Stripe integration
- [ ] Subscription tiers
- [ ] Rate limiting
- [ ] Overage handling

### Phase 3: Dashboard (Weeks 6-7)
- [ ] OAuth login (GitHub/Google)
- [ ] Usage analytics page
- [ ] API key management
- [ ] Subscription management

### Phase 4: Polish (Week 8)
- [ ] Additional models (Llama, Mistral)
- [ ] Prompt caching
- [ ] Conversation sync
- [ ] Production deployment
