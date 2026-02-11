//! Agent conversation persistence with SQLite-backed storage
//!
//! Saves full conversation history with timestamps, supports resuming
//! conversations by ID, and provides list/search capabilities.

use crate::provider::{Message, MessageContent, MessageRole};
use crate::AgentError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// Conversation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub id: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub model: String,
}

/// A stored message with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: i64,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub token_count: Option<i64>,
}

/// SQLite-backed conversation store
pub struct ConversationStore {
    conn: Mutex<Connection>,
}

impl ConversationStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AgentError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AgentError::Conversation(format!("failed to create directory: {}", e))
            })?;
        }

        let conn = Connection::open(&path).map_err(|e| {
            AgentError::Conversation(format!("failed to open database: {}", e))
        })?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.initialize_schema()?;
        Ok(store)
    }

    pub fn open_default() -> Result<Self, AgentError> {
        let path = dirs_config_path().join("conversations.db");
        Self::open(path)
    }

    pub fn open_in_memory() -> Result<Self, AgentError> {
        let conn = Connection::open_in_memory().map_err(|e| {
            AgentError::Conversation(format!("failed to open in-memory database: {}", e))
        })?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.initialize_schema()?;
        Ok(store)
    }

    fn initialize_schema(&self) -> Result<(), AgentError> {
        let conn = self.conn.lock().map_err(|e| {
            AgentError::Conversation(format!("database lock poisoned: {}", e))
        })?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                model TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                token_count INTEGER,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id)
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conversation
                ON messages(conversation_id, id);
            CREATE INDEX IF NOT EXISTS idx_conversations_updated
                ON conversations(updated_at DESC);",
        )
        .map_err(|e| {
            AgentError::Conversation(format!("failed to initialize schema: {}", e))
        })?;

        Ok(())
    }

    /// Create a new conversation and return its ID
    pub fn create_conversation(
        &self,
        id: &str,
        title: Option<&str>,
        model: &str,
    ) -> Result<(), AgentError> {
        let conn = self.conn.lock().map_err(|e| {
            AgentError::Conversation(format!("database lock poisoned: {}", e))
        })?;

        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at, model) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, title, now, now, model],
        )
        .map_err(|e| {
            AgentError::Conversation(format!("failed to create conversation: {}", e))
        })?;

        Ok(())
    }

    /// Save a message to a conversation
    pub fn save_message(
        &self,
        conversation_id: &str,
        message: &Message,
        token_count: Option<i64>,
    ) -> Result<i64, AgentError> {
        let conn = self.conn.lock().map_err(|e| {
            AgentError::Conversation(format!("database lock poisoned: {}", e))
        })?;

        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        };

        let content = match &message.content {
            MessageContent::Text(t) => t.clone(),
            MessageContent::Blocks(blocks) => serde_json::to_string(blocks)
                .unwrap_or_else(|_| "[]".to_string()),
        };

        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, timestamp, token_count) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![conversation_id, role, content, now, token_count],
        )
        .map_err(|e| {
            AgentError::Conversation(format!("failed to save message: {}", e))
        })?;

        let msg_id = conn.last_insert_rowid();

        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )
        .map_err(|e| {
            AgentError::Conversation(format!("failed to update conversation timestamp: {}", e))
        })?;

        Ok(msg_id)
    }

    /// Load all messages for a conversation
    pub fn load_messages(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<StoredMessage>, AgentError> {
        let conn = self.conn.lock().map_err(|e| {
            AgentError::Conversation(format!("database lock poisoned: {}", e))
        })?;

        let mut stmt = conn
            .prepare(
                "SELECT id, conversation_id, role, content, timestamp, token_count
                 FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY id ASC",
            )
            .map_err(|e| {
                AgentError::Conversation(format!("failed to prepare query: {}", e))
            })?;

        let messages = stmt
            .query_map(params![conversation_id], |row| {
                let ts_str: String = row.get(4)?;
                let timestamp = ts_str
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now());

                Ok(StoredMessage {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    timestamp,
                    token_count: row.get(5)?,
                })
            })
            .map_err(|e| {
                AgentError::Conversation(format!("failed to query messages: {}", e))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                AgentError::Conversation(format!("failed to collect messages: {}", e))
            })?;

        Ok(messages)
    }

    /// Convert stored messages back to provider Messages for conversation resumption
    pub fn resume_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<Message>, AgentError> {
        let stored = self.load_messages(conversation_id)?;

        let messages = stored
            .into_iter()
            .map(|sm| {
                let role = match sm.role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "system" => MessageRole::System,
                    _ => MessageRole::User,
                };

                Message {
                    role,
                    content: MessageContent::Text(sm.content),
                }
            })
            .collect();

        Ok(messages)
    }

    /// List all conversations, most recently updated first
    pub fn list_conversations(
        &self,
        limit: usize,
    ) -> Result<Vec<ConversationMeta>, AgentError> {
        let conn = self.conn.lock().map_err(|e| {
            AgentError::Conversation(format!("database lock poisoned: {}", e))
        })?;

        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.title, c.created_at, c.updated_at, c.model,
                        (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
                 FROM conversations c
                 ORDER BY c.updated_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| {
                AgentError::Conversation(format!("failed to prepare query: {}", e))
            })?;

        let conversations = stmt
            .query_map(params![limit as i64], |row| {
                let created_str: String = row.get(2)?;
                let updated_str: String = row.get(3)?;

                Ok(ConversationMeta {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: created_str
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: updated_str
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    message_count: row.get::<_, i64>(5)? as usize,
                    model: row.get(4)?,
                })
            })
            .map_err(|e| {
                AgentError::Conversation(format!("failed to query conversations: {}", e))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                AgentError::Conversation(format!("failed to collect conversations: {}", e))
            })?;

        Ok(conversations)
    }

    /// Search conversations by title or message content
    pub fn search_conversations(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ConversationMeta>, AgentError> {
        let conn = self.conn.lock().map_err(|e| {
            AgentError::Conversation(format!("database lock poisoned: {}", e))
        })?;

        let pattern = format!("%{}%", query);

        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT c.id, c.title, c.created_at, c.updated_at, c.model,
                        (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
                 FROM conversations c
                 LEFT JOIN messages m ON m.conversation_id = c.id
                 WHERE c.title LIKE ?1 OR m.content LIKE ?1
                 ORDER BY c.updated_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| {
                AgentError::Conversation(format!("failed to prepare search query: {}", e))
            })?;

        let conversations = stmt
            .query_map(params![pattern, limit as i64], |row| {
                let created_str: String = row.get(2)?;
                let updated_str: String = row.get(3)?;

                Ok(ConversationMeta {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: created_str
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: updated_str
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    message_count: row.get::<_, i64>(5)? as usize,
                    model: row.get(4)?,
                })
            })
            .map_err(|e| {
                AgentError::Conversation(format!("failed to execute search: {}", e))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                AgentError::Conversation(format!("failed to collect search results: {}", e))
            })?;

        Ok(conversations)
    }

    /// Get conversation metadata by ID
    pub fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<ConversationMeta>, AgentError> {
        let conn = self.conn.lock().map_err(|e| {
            AgentError::Conversation(format!("database lock poisoned: {}", e))
        })?;

        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.title, c.created_at, c.updated_at, c.model,
                        (SELECT COUNT(*) FROM messages WHERE conversation_id = c.id) as msg_count
                 FROM conversations c
                 WHERE c.id = ?1",
            )
            .map_err(|e| {
                AgentError::Conversation(format!("failed to prepare query: {}", e))
            })?;

        let result = stmt
            .query_row(params![conversation_id], |row| {
                let created_str: String = row.get(2)?;
                let updated_str: String = row.get(3)?;

                Ok(ConversationMeta {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: created_str
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: updated_str
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    message_count: row.get::<_, i64>(5)? as usize,
                    model: row.get(4)?,
                })
            })
            .ok();

        Ok(result)
    }

    /// Delete a conversation and all its messages
    pub fn delete_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<(), AgentError> {
        let conn = self.conn.lock().map_err(|e| {
            AgentError::Conversation(format!("database lock poisoned: {}", e))
        })?;

        conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![conversation_id],
        )
        .map_err(|e| {
            AgentError::Conversation(format!("failed to delete messages: {}", e))
        })?;

        conn.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![conversation_id],
        )
        .map_err(|e| {
            AgentError::Conversation(format!("failed to delete conversation: {}", e))
        })?;

        Ok(())
    }
}

fn dirs_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("colony")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> ConversationStore {
        ConversationStore::open_in_memory().unwrap()
    }

    #[test]
    fn test_create_and_get_conversation() {
        let store = test_store();
        store
            .create_conversation("conv-1", Some("Test Chat"), "claude-3")
            .unwrap();

        let meta = store.get_conversation("conv-1").unwrap().unwrap();
        assert_eq!(meta.id, "conv-1");
        assert_eq!(meta.title.as_deref(), Some("Test Chat"));
        assert_eq!(meta.model, "claude-3");
        assert_eq!(meta.message_count, 0);
    }

    #[test]
    fn test_save_and_load_messages() {
        let store = test_store();
        store
            .create_conversation("conv-1", None, "claude-3")
            .unwrap();

        let msg1 = Message {
            role: MessageRole::User,
            content: MessageContent::Text("Hello".to_string()),
        };
        let msg2 = Message {
            role: MessageRole::Assistant,
            content: MessageContent::Text("Hi there!".to_string()),
        };

        store.save_message("conv-1", &msg1, Some(5)).unwrap();
        store.save_message("conv-1", &msg2, Some(10)).unwrap();

        let messages = store.load_messages("conv-1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[0].token_count, Some(5));
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "Hi there!");
    }

    #[test]
    fn test_resume_conversation() {
        let store = test_store();
        store
            .create_conversation("conv-1", None, "claude-3")
            .unwrap();

        store
            .save_message(
                "conv-1",
                &Message {
                    role: MessageRole::User,
                    content: MessageContent::Text("What is Rust?".to_string()),
                },
                None,
            )
            .unwrap();
        store
            .save_message(
                "conv-1",
                &Message {
                    role: MessageRole::Assistant,
                    content: MessageContent::Text("A systems programming language.".to_string()),
                },
                None,
            )
            .unwrap();

        let messages = store.resume_conversation("conv-1").unwrap();
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, MessageRole::User));
        assert!(matches!(messages[1].role, MessageRole::Assistant));
    }

    #[test]
    fn test_list_conversations() {
        let store = test_store();
        store
            .create_conversation("conv-1", Some("First"), "claude-3")
            .unwrap();
        store
            .create_conversation("conv-2", Some("Second"), "gpt-4")
            .unwrap();

        let list = store.list_conversations(10).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_search_conversations() {
        let store = test_store();
        store
            .create_conversation("conv-1", Some("Rust help"), "claude-3")
            .unwrap();
        store
            .create_conversation("conv-2", Some("Python help"), "gpt-4")
            .unwrap();

        store
            .save_message(
                "conv-1",
                &Message {
                    role: MessageRole::User,
                    content: MessageContent::Text("How do I use cargo?".to_string()),
                },
                None,
            )
            .unwrap();

        let results = store.search_conversations("Rust", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "conv-1");

        let results = store.search_conversations("cargo", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_delete_conversation() {
        let store = test_store();
        store
            .create_conversation("conv-1", None, "claude-3")
            .unwrap();
        store
            .save_message(
                "conv-1",
                &Message {
                    role: MessageRole::User,
                    content: MessageContent::Text("test".to_string()),
                },
                None,
            )
            .unwrap();

        store.delete_conversation("conv-1").unwrap();
        assert!(store.get_conversation("conv-1").unwrap().is_none());
        assert!(store.load_messages("conv-1").unwrap().is_empty());
    }

    #[test]
    fn test_message_count_updates() {
        let store = test_store();
        store
            .create_conversation("conv-1", None, "claude-3")
            .unwrap();

        store
            .save_message(
                "conv-1",
                &Message {
                    role: MessageRole::User,
                    content: MessageContent::Text("msg1".to_string()),
                },
                None,
            )
            .unwrap();
        store
            .save_message(
                "conv-1",
                &Message {
                    role: MessageRole::Assistant,
                    content: MessageContent::Text("msg2".to_string()),
                },
                None,
            )
            .unwrap();

        let meta = store.get_conversation("conv-1").unwrap().unwrap();
        assert_eq!(meta.message_count, 2);
    }

    #[test]
    fn test_get_nonexistent_conversation() {
        let store = test_store();
        assert!(store.get_conversation("nonexistent").unwrap().is_none());
    }
}
