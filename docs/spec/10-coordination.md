# Part 10: Coordination

## 10.1 Overview

Colony provides coordination primitives for multi-agent and multi-session workflows:
- **Tuple Space**: Linda-style shared memory for decoupled communication
- **Inter-Session Messaging**: Direct message passing between shell sessions
- **Signals and Events**: Event-driven coordination

## 10.2 Tuple Space

The tuple space is a shared associative memory where processes can:
- **out**: Add a tuple
- **in**: Remove a matching tuple (blocking)
- **rd**: Read a matching tuple without removing (blocking)
- **inp**: Non-blocking in
- **rdp**: Non-blocking rd

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            Tuple Space                                   │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  ("task", "build", {project: "colony", status: "pending"})      │   │
│  │  ("task", "test", {project: "colony", status: "running"})       │   │
│  │  ("result", "build-123", {success: true, artifacts: [...]})     │   │
│  │  ("lock", "deploy", {owner: "session-abc", expires: "..."})     │   │
│  │  ("event", "file-changed", {path: "/src/main.rs"})              │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Session A ──out──▶ ◀──in─── Session B                                 │
│  Session C ──rd───▶                                                     │
│  Agent X ───out──▶ ◀──in─── Agent Y                                    │
└─────────────────────────────────────────────────────────────────────────┘
```

### 10.2.1 Tuple Structure

```rust
// crates/colony-tuple-space/src/tuple.rs

use colony_core::types::Value;

/// A tuple in the tuple space
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tuple {
    /// Tuple fields
    pub fields: Vec<Value>,
    /// When this tuple was created
    pub created_at: DateTime<Utc>,
    /// Optional expiration time
    pub expires_at: Option<DateTime<Utc>>,
    /// Source identifier (session, agent, etc.)
    pub source: Option<String>,
    /// Unique tuple ID
    pub id: String,
}

impl Tuple {
    pub fn new(fields: Vec<Value>) -> Self {
        Self {
            fields,
            created_at: Utc::now(),
            expires_at: None,
            source: None,
            id: generate_id("tup"),
        }
    }
    
    pub fn with_expiry(mut self, duration: Duration) -> Self {
        self.expires_at = Some(Utc::now() + chrono::Duration::from_std(duration).unwrap());
        self
    }
    
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }
    
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|e| Utc::now() > e).unwrap_or(false)
    }
    
    /// Get field by index
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.fields.get(index)
    }
    
    /// Get first field (often used as tuple type)
    pub fn tag(&self) -> Option<&Value> {
        self.fields.first()
    }
}

/// Pattern for matching tuples
#[derive(Clone, Debug)]
pub struct TuplePattern {
    pub fields: Vec<FieldPattern>,
}

/// Pattern for a single field
#[derive(Clone, Debug)]
pub enum FieldPattern {
    /// Match any value
    Any,
    /// Match exact value
    Exact(Value),
    /// Match by type
    Type(TypeMatcher),
    /// Match with predicate
    Predicate(PredicateFn),
    /// Capture into variable
    Capture(String),
    /// Match with guard expression
    Guard {
        pattern: Box<FieldPattern>,
        guard: String,  // Expression string
    },
}

#[derive(Clone, Debug)]
pub enum TypeMatcher {
    Null,
    Bool,
    Int,
    Float,
    Str,
    List,
    Dict,
    Any,
}

pub type PredicateFn = Arc<dyn Fn(&Value) -> bool + Send + Sync>;

impl TuplePattern {
    pub fn new(fields: Vec<FieldPattern>) -> Self {
        Self { fields }
    }
    
    /// Check if a tuple matches this pattern
    pub fn matches(&self, tuple: &Tuple) -> bool {
        if self.fields.len() != tuple.fields.len() {
            return false;
        }
        
        self.fields.iter()
            .zip(tuple.fields.iter())
            .all(|(pattern, value)| pattern.matches(value))
    }
    
    /// Extract captures from a matching tuple
    pub fn extract(&self, tuple: &Tuple) -> IndexMap<String, Value> {
        let mut captures = IndexMap::new();
        
        for (pattern, value) in self.fields.iter().zip(tuple.fields.iter()) {
            if let FieldPattern::Capture(name) = pattern {
                captures.insert(name.clone(), value.clone());
            }
        }
        
        captures
    }
}

impl FieldPattern {
    pub fn matches(&self, value: &Value) -> bool {
        match self {
            FieldPattern::Any => true,
            FieldPattern::Exact(expected) => value == expected,
            FieldPattern::Type(matcher) => matcher.matches(value),
            FieldPattern::Predicate(f) => f(value),
            FieldPattern::Capture(_) => true,  // Captures match anything
            FieldPattern::Guard { pattern, .. } => {
                // Would need expression evaluation
                pattern.matches(value)
            }
        }
    }
}

impl TypeMatcher {
    pub fn matches(&self, value: &Value) -> bool {
        match (self, value) {
            (TypeMatcher::Any, _) => true,
            (TypeMatcher::Null, Value::Null) => true,
            (TypeMatcher::Bool, Value::Bool(_)) => true,
            (TypeMatcher::Int, Value::Int(_)) => true,
            (TypeMatcher::Float, Value::Float(_)) => true,
            (TypeMatcher::Str, Value::Str(_)) => true,
            (TypeMatcher::List, Value::List(_)) => true,
            (TypeMatcher::Dict, Value::Dict(_)) => true,
            _ => false,
        }
    }
}
```

### 10.2.2 Tuple Space Implementation

```rust
// crates/colony-tuple-space/src/space.rs

use std::collections::VecDeque;
use tokio::sync::{RwLock, Notify};

/// The tuple space
pub struct TupleSpace {
    /// All tuples
    tuples: RwLock<Vec<Tuple>>,
    /// Waiters for blocking operations
    waiters: RwLock<Vec<Waiter>>,
    /// Notification for new tuples
    notify: Notify,
    /// Configuration
    config: TupleSpaceConfig,
}

struct Waiter {
    id: String,
    pattern: TuplePattern,
    sender: tokio::sync::oneshot::Sender<Tuple>,
    remove: bool,  // true for 'in', false for 'rd'
}

#[derive(Clone, Debug)]
pub struct TupleSpaceConfig {
    /// Maximum number of tuples
    pub max_tuples: usize,
    /// Cleanup interval for expired tuples
    pub cleanup_interval: Duration,
    /// Default tuple expiry
    pub default_expiry: Option<Duration>,
}

impl Default for TupleSpaceConfig {
    fn default() -> Self {
        Self {
            max_tuples: 10000,
            cleanup_interval: Duration::from_secs(60),
            default_expiry: None,
        }
    }
}

impl TupleSpace {
    pub fn new(config: TupleSpaceConfig) -> Arc<Self> {
        let space = Arc::new(Self {
            tuples: RwLock::new(Vec::new()),
            waiters: RwLock::new(Vec::new()),
            notify: Notify::new(),
            config,
        });
        
        // Start cleanup task
        let space_clone = space.clone();
        tokio::spawn(async move {
            space_clone.cleanup_loop().await;
        });
        
        space
    }
    
    /// Add a tuple to the space (out)
    pub async fn out(&self, tuple: Tuple) -> Result<(), TupleSpaceError> {
        // Check for matching waiters first
        {
            let mut waiters = self.waiters.write().await;
            let mut i = 0;
            while i < waiters.len() {
                if waiters[i].pattern.matches(&tuple) {
                    let waiter = waiters.remove(i);
                    let _ = waiter.sender.send(tuple.clone());
                    
                    if waiter.remove {
                        // 'in' operation - tuple is consumed
                        return Ok(());
                    }
                } else {
                    i += 1;
                }
            }
        }
        
        // No waiter, add to space
        let mut tuples = self.tuples.write().await;
        
        if tuples.len() >= self.config.max_tuples {
            return Err(TupleSpaceError::SpaceFull);
        }
        
        tuples.push(tuple);
        self.notify.notify_waiters();
        
        Ok(())
    }
    
    /// Remove a matching tuple (blocking)
    pub async fn r#in(&self, pattern: TuplePattern) -> Tuple {
        self.take_or_wait(pattern, true).await
    }
    
    /// Read a matching tuple without removing (blocking)
    pub async fn rd(&self, pattern: TuplePattern) -> Tuple {
        self.take_or_wait(pattern, false).await
    }
    
    /// Non-blocking in
    pub async fn inp(&self, pattern: TuplePattern) -> Option<Tuple> {
        let mut tuples = self.tuples.write().await;
        
        for i in 0..tuples.len() {
            if !tuples[i].is_expired() && pattern.matches(&tuples[i]) {
                return Some(tuples.remove(i));
            }
        }
        
        None
    }
    
    /// Non-blocking rd
    pub async fn rdp(&self, pattern: TuplePattern) -> Option<Tuple> {
        let tuples = self.tuples.read().await;
        
        for tuple in tuples.iter() {
            if !tuple.is_expired() && pattern.matches(tuple) {
                return Some(tuple.clone());
            }
        }
        
        None
    }
    
    /// Remove a matching tuple with timeout
    pub async fn in_timeout(&self, pattern: TuplePattern, timeout: Duration) -> Option<Tuple> {
        tokio::time::timeout(timeout, self.r#in(pattern))
            .await
            .ok()
    }
    
    /// Read a matching tuple with timeout
    pub async fn rd_timeout(&self, pattern: TuplePattern, timeout: Duration) -> Option<Tuple> {
        tokio::time::timeout(timeout, self.rd(pattern))
            .await
            .ok()
    }
    
    async fn take_or_wait(&self, pattern: TuplePattern, remove: bool) -> Tuple {
        // First, try to find a matching tuple
        {
            let mut tuples = self.tuples.write().await;
            
            for i in 0..tuples.len() {
                if !tuples[i].is_expired() && pattern.matches(&tuples[i]) {
                    if remove {
                        return tuples.remove(i);
                    } else {
                        return tuples[i].clone();
                    }
                }
            }
        }
        
        // No match, register as waiter
        let (sender, receiver) = tokio::sync::oneshot::channel();
        
        {
            let mut waiters = self.waiters.write().await;
            waiters.push(Waiter {
                id: generate_id("wait"),
                pattern,
                sender,
                remove,
            });
        }
        
        // Wait for tuple
        receiver.await.unwrap()
    }
    
    /// Query all matching tuples (non-destructive)
    pub async fn query(&self, pattern: TuplePattern) -> Vec<Tuple> {
        let tuples = self.tuples.read().await;
        
        tuples.iter()
            .filter(|t| !t.is_expired() && pattern.matches(t))
            .cloned()
            .collect()
    }
    
    /// Count matching tuples
    pub async fn count(&self, pattern: TuplePattern) -> usize {
        let tuples = self.tuples.read().await;
        
        tuples.iter()
            .filter(|t| !t.is_expired() && pattern.matches(t))
            .count()
    }
    
    /// Get total tuple count
    pub async fn len(&self) -> usize {
        self.tuples.read().await.len()
    }
    
    /// Clear all tuples
    pub async fn clear(&self) {
        let mut tuples = self.tuples.write().await;
        tuples.clear();
    }
    
    async fn cleanup_loop(&self) {
        loop {
            tokio::time::sleep(self.config.cleanup_interval).await;
            self.cleanup_expired().await;
        }
    }
    
    async fn cleanup_expired(&self) {
        let mut tuples = self.tuples.write().await;
        tuples.retain(|t| !t.is_expired());
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TupleSpaceError {
    #[error("Tuple space is full")]
    SpaceFull,
    #[error("Operation timed out")]
    Timeout,
    #[error("Invalid pattern")]
    InvalidPattern,
}
```

### 10.2.3 Tuple Space Commands

```colony
# Add a tuple
ts:out ("task", "build", {project: "colony"})

# Add with expiry
ts:out ("lock", "deploy", {owner: $SESSION_ID}) --expires 5m

# Blocking take (waits for match)
var task = ts:in ("task", _, _)
echo "Got task: $task"

# Blocking read (doesn't remove)
var lock = ts:rd ("lock", "deploy", _)

# Non-blocking variants
if var task = ts:inp ("task", _, _) {
    echo "Found task: $task"
} else {
    echo "No tasks available"
}

# With timeout
var task = ts:in ("task", _, _) --timeout 10s
if $task {
    echo "Got task"
} else {
    echo "Timed out waiting for task"
}

# Query all matching
var tasks = ts:query ("task", _, {status: "pending"})
for task in $tasks {
    echo "Pending: $task"
}

# Count matching
var pending = ts:count ("task", _, {status: "pending"})
echo "Pending tasks: $pending"

# Pattern with type matching
var result = ts:in ("result", :Str, :Dict)

# Pattern with capture
var (_, id, data) = ts:in ("result", $id, $data)
echo "Result $id: $data"

# Clear space
ts:clear

# Space stats
ts:info
```

## 10.3 Inter-Session Messaging

### 10.3.1 Message Types

```rust
// crates/colony-tuple-space/src/messaging.rs

/// Message between sessions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: Option<String>,
    pub body: Value,
    pub timestamp: DateTime<Utc>,
    pub reply_to: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Message bus for inter-session communication
pub struct MessageBus {
    tuple_space: Arc<TupleSpace>,
}

impl MessageBus {
    pub fn new(tuple_space: Arc<TupleSpace>) -> Self {
        Self { tuple_space }
    }
    
    /// Send a message to a session
    pub async fn send(&self, to: &str, body: Value, from: &str) -> Result<String, MessagingError> {
        let msg = SessionMessage {
            id: generate_id("msg"),
            from: from.to_string(),
            to: to.to_string(),
            subject: None,
            body,
            timestamp: Utc::now(),
            reply_to: None,
            expires_at: None,
        };
        
        let tuple = Tuple::new(vec![
            Value::Str("message".to_string()),
            Value::Str(to.to_string()),
            Value::Str(serde_json::to_string(&msg).unwrap()),
        ]);
        
        self.tuple_space.out(tuple).await?;
        
        Ok(msg.id)
    }
    
    /// Receive messages for a session (blocking)
    pub async fn receive(&self, session_id: &str) -> SessionMessage {
        let pattern = TuplePattern::new(vec![
            FieldPattern::Exact(Value::Str("message".to_string())),
            FieldPattern::Exact(Value::Str(session_id.to_string())),
            FieldPattern::Any,
        ]);
        
        let tuple = self.tuple_space.r#in(pattern).await;
        
        let msg_str = tuple.fields[2].as_str().unwrap();
        serde_json::from_str(msg_str).unwrap()
    }
    
    /// Check for messages (non-blocking)
    pub async fn poll(&self, session_id: &str) -> Option<SessionMessage> {
        let pattern = TuplePattern::new(vec![
            FieldPattern::Exact(Value::Str("message".to_string())),
            FieldPattern::Exact(Value::Str(session_id.to_string())),
            FieldPattern::Any,
        ]);
        
        let tuple = self.tuple_space.inp(pattern).await?;
        
        let msg_str = tuple.fields[2].as_str().unwrap();
        Some(serde_json::from_str(msg_str).unwrap())
    }
    
    /// Broadcast to multiple sessions
    pub async fn broadcast(&self, to: &[String], body: Value, from: &str) -> Result<Vec<String>, MessagingError> {
        let mut ids = Vec::new();
        
        for recipient in to {
            let id = self.send(recipient, body.clone(), from).await?;
            ids.push(id);
        }
        
        Ok(ids)
    }
    
    /// Request-response pattern
    pub async fn request(&self, to: &str, body: Value, from: &str, timeout: Duration) -> Result<SessionMessage, MessagingError> {
        let request_id = self.send(to, body, from).await?;
        
        // Wait for reply
        let pattern = TuplePattern::new(vec![
            FieldPattern::Exact(Value::Str("message".to_string())),
            FieldPattern::Exact(Value::Str(from.to_string())),
            FieldPattern::Predicate(Arc::new(move |v| {
                if let Value::Str(s) = v {
                    if let Ok(msg) = serde_json::from_str::<SessionMessage>(s) {
                        return msg.reply_to.as_ref() == Some(&request_id);
                    }
                }
                false
            })),
        ]);
        
        let tuple = self.tuple_space.in_timeout(pattern, timeout).await
            .ok_or(MessagingError::Timeout)?;
        
        let msg_str = tuple.fields[2].as_str().unwrap();
        Ok(serde_json::from_str(msg_str).unwrap())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MessagingError {
    #[error("Tuple space error: {0}")]
    TupleSpace(#[from] TupleSpaceError),
    #[error("Request timed out")]
    Timeout,
    #[error("Session not found: {0}")]
    SessionNotFound(String),
}
```

### 10.3.2 Messaging Commands

```colony
# Send message to another session
msg:send "session-abc" "Hello from here!"

# Send structured message
msg:send "session-abc" {
    type: "task"
    data: { action: "build", target: "all" }
}

# Receive message (blocking)
var msg = msg:receive
echo "From: $msg.from"
echo "Body: $msg.body"

# Poll for messages (non-blocking)
if var msg = msg:poll {
    echo "Got message: $msg.body"
}

# Receive with timeout
var msg = msg:receive --timeout 30s

# Broadcast to multiple sessions
msg:broadcast ["session-a", "session-b", "session-c"] "Build complete!"

# Request-response
var response = msg:request "worker-session" {
    command: "status"
} --timeout 10s
echo "Worker status: $response.body"

# Reply to a message
msg:reply $msg.id "Acknowledged"

# List active sessions
msg:sessions

# Get current session ID
echo $SESSION_ID
```

## 10.4 Event System

```rust
// crates/colony-tuple-space/src/events.rs

/// Event system built on tuple space
pub struct EventSystem {
    tuple_space: Arc<TupleSpace>,
}

impl EventSystem {
    pub fn new(tuple_space: Arc<TupleSpace>) -> Self {
        Self { tuple_space }
    }
    
    /// Emit an event
    pub async fn emit(&self, event_type: &str, data: Value) -> Result<(), TupleSpaceError> {
        let tuple = Tuple::new(vec![
            Value::Str("event".to_string()),
            Value::Str(event_type.to_string()),
            data,
            Value::Str(Utc::now().to_rfc3339()),
        ]).with_expiry(Duration::from_secs(60));  // Events expire
        
        self.tuple_space.out(tuple).await
    }
    
    /// Wait for an event
    pub async fn wait(&self, event_type: &str) -> (Value, DateTime<Utc>) {
        let pattern = TuplePattern::new(vec![
            FieldPattern::Exact(Value::Str("event".to_string())),
            FieldPattern::Exact(Value::Str(event_type.to_string())),
            FieldPattern::Any,
            FieldPattern::Any,
        ]);
        
        let tuple = self.tuple_space.rd(pattern).await;
        
        let data = tuple.fields[2].clone();
        let timestamp = tuple.fields[3].as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        
        (data, timestamp)
    }
    
    /// Subscribe to events (returns stream)
    pub fn subscribe(&self, event_type: &str) -> impl Stream<Item = (Value, DateTime<Utc>)> {
        let pattern = TuplePattern::new(vec![
            FieldPattern::Exact(Value::Str("event".to_string())),
            FieldPattern::Exact(Value::Str(event_type.to_string())),
            FieldPattern::Any,
            FieldPattern::Any,
        ]);
        
        let space = self.tuple_space.clone();
        
        async_stream::stream! {
            loop {
                let tuple = space.rd(pattern.clone()).await;
                
                let data = tuple.fields[2].clone();
                let timestamp = tuple.fields[3].as_str()
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now);
                
                yield (data, timestamp);
                
                // Small delay to prevent tight loop
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}
```

### 10.4.1 Event Commands

```colony
# Emit an event
event:emit "build-complete" {
    project: "colony"
    duration: 45
    success: true
}

# Wait for an event
var evt = event:wait "build-complete"
echo "Build took $evt.data.duration seconds"

# Wait with timeout
var evt = event:wait "deploy-ready" --timeout 5m

# Subscribe to events (in background)
event:subscribe "file-changed" {
    echo "File changed: $event.data.path"
    # Trigger rebuild
    make build
} &

# Emit file system events
watch src/ | while read path event {
    event:emit "file-changed" {path: $path, event: $event}
}
```

## 10.5 Distributed Coordination

### 10.5.1 Locks

```rust
// crates/colony-tuple-space/src/locks.rs

/// Distributed lock implementation
pub struct DistributedLock {
    tuple_space: Arc<TupleSpace>,
    name: String,
    owner: String,
    held: AtomicBool,
}

impl DistributedLock {
    pub fn new(tuple_space: Arc<TupleSpace>, name: &str, owner: &str) -> Self {
        Self {
            tuple_space,
            name: name.to_string(),
            owner: owner.to_string(),
            held: AtomicBool::new(false),
        }
    }
    
    /// Try to acquire the lock
    pub async fn try_acquire(&self) -> bool {
        // Try to take existing lock
        let pattern = TuplePattern::new(vec![
            FieldPattern::Exact(Value::Str("lock".to_string())),
            FieldPattern::Exact(Value::Str(self.name.clone())),
            FieldPattern::Any,
        ]);
        
        // Check if lock exists
        if let Some(existing) = self.tuple_space.rdp(pattern.clone()).await {
            // Lock exists - check if it's ours or expired
            if existing.is_expired() {
                // Expired, remove it
                self.tuple_space.inp(pattern).await;
            } else {
                return false;
            }
        }
        
        // Try to place our lock
        let lock_tuple = Tuple::new(vec![
            Value::Str("lock".to_string()),
            Value::Str(self.name.clone()),
            Value::Str(self.owner.clone()),
        ]).with_expiry(Duration::from_secs(30));  // Auto-expire
        
        if self.tuple_space.out(lock_tuple).await.is_ok() {
            self.held.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
    
    /// Acquire the lock (blocking)
    pub async fn acquire(&self) {
        while !self.try_acquire().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    /// Acquire with timeout
    pub async fn acquire_timeout(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        
        while Instant::now() < deadline {
            if self.try_acquire().await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        false
    }
    
    /// Release the lock
    pub async fn release(&self) {
        if !self.held.load(Ordering::SeqCst) {
            return;
        }
        
        let pattern = TuplePattern::new(vec![
            FieldPattern::Exact(Value::Str("lock".to_string())),
            FieldPattern::Exact(Value::Str(self.name.clone())),
            FieldPattern::Exact(Value::Str(self.owner.clone())),
        ]);
        
        self.tuple_space.inp(pattern).await;
        self.held.store(false, Ordering::SeqCst);
    }
    
    /// Extend lock expiry
    pub async fn refresh(&self) -> bool {
        if !self.held.load(Ordering::SeqCst) {
            return false;
        }
        
        // Remove and re-add with fresh expiry
        self.release().await;
        self.try_acquire().await
    }
}
```

### 10.5.2 Lock Commands

```colony
# Acquire a lock
lock:acquire "deploy"
    # Critical section
    deploy-to-prod
lock:release "deploy"

# With automatic release (using block)
lock:with "deploy" {
    deploy-to-prod
}

# Try to acquire (non-blocking)
if lock:try "deploy" {
    deploy-to-prod
    lock:release "deploy"
} else {
    echo "Deployment already in progress"
}

# With timeout
if lock:acquire "deploy" --timeout 30s {
    deploy-to-prod
    lock:release "deploy"
} else {
    echo "Couldn't acquire lock"
}

# Check lock status
var info = lock:info "deploy"
if $info {
    echo "Lock held by: $info.owner"
    echo "Expires: $info.expires"
}
```

### 10.5.3 Barriers and Semaphores

```colony
# Barrier - wait for N participants
barrier:create "sync-point" --count 3

# In each participant:
barrier:wait "sync-point"
echo "All participants ready!"

# Semaphore
semaphore:create "db-connections" --permits 5

semaphore:acquire "db-connections"
    query-database
semaphore:release "db-connections"

# With block (auto-release)
semaphore:with "db-connections" {
    query-database
}
```

---

*Continue to Part 11: MCP Integration*