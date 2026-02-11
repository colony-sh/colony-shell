use crate::error::Result;
use crate::query::MetadataQuery;
use crate::scope::Scope;
use crate::store::{MemoryStore, SearchResult};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationRole {
    User,
    Assistant,
}

impl ConversationRole {
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

impl std::fmt::Display for ConversationRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub role: ConversationRole,
    pub content: String,
    pub memory_id: String,
}

pub struct MemoryBackedContext {
    store: Arc<MemoryStore>,
    scope: Scope,
}

impl MemoryBackedContext {
    pub fn new(store: Arc<MemoryStore>, scope: Scope) -> Self {
        Self { store, scope }
    }

    pub fn store(&self) -> &MemoryStore {
        &self.store
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    pub async fn store_turn(
        &self,
        role: ConversationRole,
        content: &str,
    ) -> Result<String> {
        self.store_turn_with_importance(role, content, 1.0).await
    }

    pub async fn store_turn_with_importance(
        &self,
        role: ConversationRole,
        content: &str,
        importance: f32,
    ) -> Result<String> {
        let metadata = json!({
            "type": "conversation_turn",
            "role": role.as_str(),
        });
        self.store
            .store_with_importance(content, self.scope.clone(), importance, Some(metadata))
            .await
    }

    pub async fn retrieve_recent(&self, limit: usize) -> Result<Vec<ConversationTurn>> {
        let results = self
            .store
            .search_in_scope("", limit, Some(&self.scope))
            .await?;
        Ok(results_to_turns(results))
    }

    pub async fn retrieve_relevant(
        &self,
        prompt: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        self.store
            .search_in_scope(prompt, limit, Some(&self.scope))
            .await
    }

    pub async fn retrieve_relevant_filtered(
        &self,
        prompt: &str,
        limit: usize,
        query: &MetadataQuery,
    ) -> Result<Vec<SearchResult>> {
        let over_fetch = limit * 3;
        let results = self
            .store
            .search_in_scope(prompt, over_fetch, Some(&self.scope))
            .await?;

        let mut filtered = Vec::with_capacity(limit);
        for result in results {
            if query.matches(&result.entry)? {
                filtered.push(result);
                if filtered.len() >= limit {
                    break;
                }
            }
        }
        Ok(filtered)
    }

    pub async fn clear(&self) -> Result<usize> {
        self.store.forget_scope(&self.scope).await
    }
}

fn results_to_turns(results: Vec<SearchResult>) -> Vec<ConversationTurn> {
    results
        .into_iter()
        .filter_map(|r| {
            let role = r
                .entry
                .metadata
                .as_ref()
                .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                .and_then(|v| v.get("role")?.as_str().map(String::from))
                .and_then(|r| match r.as_str() {
                    "user" => Some(ConversationRole::User),
                    "assistant" => Some(ConversationRole::Assistant),
                    _ => None,
                });

            role.map(|role| ConversationTurn {
                role,
                content: r.entry.content.clone(),
                memory_id: r.entry.id.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_role_display() {
        assert_eq!(ConversationRole::User.as_str(), "user");
        assert_eq!(ConversationRole::Assistant.as_str(), "assistant");
        assert_eq!(format!("{}", ConversationRole::User), "user");
    }

    #[tokio::test]
    async fn test_store_and_retrieve_turn() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());
        let ctx = MemoryBackedContext::new(store, Scope::Session("test-session".to_string()));

        let id = ctx
            .store_turn(ConversationRole::User, "What is Rust?")
            .await
            .unwrap();
        assert!(!id.is_empty());

        let entry = ctx.store().get(&id).await.unwrap().unwrap();
        assert_eq!(entry.content, "What is Rust?");

        let meta: serde_json::Value =
            serde_json::from_str(entry.metadata.as_ref().unwrap()).unwrap();
        assert_eq!(meta["role"], "user");
        assert_eq!(meta["type"], "conversation_turn");
    }

    #[tokio::test]
    async fn test_store_multiple_turns() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());
        let ctx = MemoryBackedContext::new(store, Scope::Session("test-session".to_string()));

        ctx.store_turn(ConversationRole::User, "Hello")
            .await
            .unwrap();
        ctx.store_turn(ConversationRole::Assistant, "Hi there!")
            .await
            .unwrap();
        ctx.store_turn(ConversationRole::User, "How does borrowing work in Rust?")
            .await
            .unwrap();

        let turns = ctx.retrieve_recent(10).await.unwrap();
        assert_eq!(turns.len(), 3);
    }

    #[tokio::test]
    async fn test_relevance_search() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());
        let ctx = MemoryBackedContext::new(store, Scope::Session("test-session".to_string()));

        ctx.store_turn(
            ConversationRole::User,
            "How does memory management work in Rust?",
        )
        .await
        .unwrap();
        ctx.store_turn(
            ConversationRole::Assistant,
            "Rust uses ownership and borrowing for memory safety.",
        )
        .await
        .unwrap();
        ctx.store_turn(
            ConversationRole::User,
            "What is the weather like today?",
        )
        .await
        .unwrap();

        let results = ctx.retrieve_relevant("ownership and borrowing", 2).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_clear_context() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());
        let ctx = MemoryBackedContext::new(store.clone(), Scope::Session("test-session".to_string()));

        ctx.store_turn(ConversationRole::User, "Hello")
            .await
            .unwrap();
        ctx.store_turn(ConversationRole::Assistant, "Hi!")
            .await
            .unwrap();

        let deleted = ctx.clear().await.unwrap();
        assert_eq!(deleted, 2);

        let turns = ctx.retrieve_recent(10).await.unwrap();
        assert!(turns.is_empty());
    }

    #[tokio::test]
    async fn test_scoped_isolation() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());

        let ctx1 = MemoryBackedContext::new(store.clone(), Scope::Session("session-1".to_string()));
        let ctx2 = MemoryBackedContext::new(store.clone(), Scope::Session("session-2".to_string()));

        ctx1.store_turn(ConversationRole::User, "Session 1 message")
            .await
            .unwrap();
        ctx2.store_turn(ConversationRole::User, "Session 2 message")
            .await
            .unwrap();

        let turns1 = ctx1.retrieve_recent(10).await.unwrap();
        let turns2 = ctx2.retrieve_recent(10).await.unwrap();

        assert_eq!(turns1.len(), 1);
        assert_eq!(turns2.len(), 1);
        assert_eq!(turns1[0].content, "Session 1 message");
        assert_eq!(turns2[0].content, "Session 2 message");
    }

    #[tokio::test]
    async fn test_store_turn_with_importance() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());
        let ctx = MemoryBackedContext::new(store.clone(), Scope::Session("test-session".to_string()));

        let id = ctx
            .store_turn_with_importance(ConversationRole::User, "Important message", 0.8)
            .await
            .unwrap();

        let entry = store.get(&id).await.unwrap().unwrap();
        assert!((entry.importance - 0.8).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_retrieve_relevant_filtered() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());
        let ctx = MemoryBackedContext::new(store, Scope::Session("test-session".to_string()));

        ctx.store_turn(ConversationRole::User, "How does Rust handle memory?")
            .await
            .unwrap();
        ctx.store_turn(
            ConversationRole::Assistant,
            "Rust uses ownership for memory management.",
        )
        .await
        .unwrap();

        let query = MetadataQuery::equals("role", json!("user"));
        let results = ctx
            .retrieve_relevant_filtered("memory", 5, &query)
            .await
            .unwrap();

        for result in &results {
            let meta: serde_json::Value =
                serde_json::from_str(result.entry.metadata.as_ref().unwrap()).unwrap();
            assert_eq!(meta["role"], "user");
        }
    }

    #[test]
    fn test_scope_accessor() {
        let store = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let dir = tempfile::tempdir().unwrap();
            rt.block_on(async { Arc::new(MemoryStore::open(dir.path()).await.unwrap()) })
        };
        let scope = Scope::Session("my-session".to_string());
        let ctx = MemoryBackedContext::new(store, scope.clone());
        assert_eq!(ctx.scope(), &scope);
    }
}
