use crate::embedding::{EmbeddingProvider, FastEmbedProvider, EMBEDDING_DIM};
use crate::error::{MemoryError, Result};
use crate::eviction::EvictionPolicy;
use crate::query::MetadataQuery;
use crate::scope::Scope;
use crate::temperature::{MemoryTemperature, TemperatureConfig};

use arrow_array::types::Float32Type;
use arrow_array::Array;
use arrow_array::{
    FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{connect, Connection, Table};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

const TABLE_NAME: &str = "memories";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub scope_type: String,
    pub scope_id: Option<String>,
    pub importance: f32,
    pub created_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,
    pub access_count: i32,
    pub metadata: Option<String>,
}

impl MemoryEntry {
    pub fn scope(&self) -> Option<Scope> {
        Scope::from_parts(&self.scope_type, self.scope_id.as_deref())
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub entry: MemoryEntry,
    pub distance: f32,
}

pub struct MemoryStore {
    db: Connection,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl MemoryStore {
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        std::fs::create_dir_all(&path)?;
        let uri = path.to_string_lossy().to_string();
        let db = connect(&uri).execute().await?;
        let embedder = Arc::new(FastEmbedProvider::new()?);

        let store = Self { db, embedder };
        store.ensure_table().await?;
        Ok(store)
    }

    pub async fn open_default() -> Result<Self> {
        let path = dirs_config_path().join("memories.lance");
        Self::open(path).await
    }

    pub async fn open_with_embedder(
        path: impl Into<PathBuf>,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self> {
        let path = path.into();
        std::fs::create_dir_all(&path)?;
        let uri = path.to_string_lossy().to_string();
        let db = connect(&uri).execute().await?;

        let store = Self { db, embedder };
        store.ensure_table().await?;
        Ok(store)
    }

    async fn ensure_table(&self) -> Result<()> {
        let table_names = self.db.table_names().execute().await?;
        if !table_names.contains(&TABLE_NAME.to_string()) {
            self.create_table().await?;
        }
        Ok(())
    }

    async fn create_table(&self) -> Result<Table> {
        let schema = memory_schema();
        self.db
            .create_empty_table(TABLE_NAME, schema)
            .execute()
            .await
            .map_err(Into::into)
    }

    async fn get_table(&self) -> Result<Table> {
        self.db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(Into::into)
    }

    pub async fn store(
        &self,
        content: &str,
        scope: Scope,
        metadata: Option<serde_json::Value>,
    ) -> Result<String> {
        self.store_with_importance(content, scope, 1.0, metadata)
            .await
    }

    pub async fn store_with_importance(
        &self,
        content: &str,
        scope: Scope,
        importance: f32,
        metadata: Option<serde_json::Value>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let embeddings = self.embedder.embed(&[content])?;
        let embedding = &embeddings[0];

        let table = self.get_table().await?;
        let batch =
            create_record_batch(&id, content, embedding, &scope, importance, now, metadata)?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], memory_schema());
        table.add(Box::new(batches)).execute().await?;

        Ok(id)
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        self.search_in_scope(query, limit, None).await
    }

    pub async fn search_in_scope(
        &self,
        query: &str,
        limit: usize,
        scope: Option<&Scope>,
    ) -> Result<Vec<SearchResult>> {
        let embeddings = self.embedder.embed(&[query])?;
        let query_vec: Vec<f32> = embeddings.into_iter().next().unwrap();

        let table = self.get_table().await?;

        let mut query_builder = table.query().nearest_to(query_vec.as_slice())?;

        if let Some(scope) = scope {
            let filter = match scope {
                Scope::Global => "scope_type = 'global'".to_string(),
                Scope::Project(id) => {
                    format!("scope_type = 'project' AND scope_id = '{}'", id)
                }
                Scope::Session(id) => {
                    format!("scope_type = 'session' AND scope_id = '{}'", id)
                }
                Scope::Task(id) => {
                    format!("scope_type = 'task' AND scope_id = '{}'", id)
                }
            };
            query_builder = query_builder.only_if(filter);
        }

        let results = query_builder
            .limit(limit)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        parse_search_results(results)
    }

    pub async fn get(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let table = self.get_table().await?;
        let filter = format!("id = '{}'", id);

        let results = table
            .query()
            .only_if(filter)
            .limit(1)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        if results.is_empty() {
            return Ok(None);
        }

        let entries = parse_record_batches(results)?;
        Ok(entries.into_iter().next())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let table = self.get_table().await?;
        table.delete(&format!("id = '{}'", id)).await?;
        Ok(())
    }

    pub async fn forget_scope(&self, scope: &Scope) -> Result<usize> {
        let table = self.get_table().await?;
        let filter = match scope {
            Scope::Global => "scope_type = 'global'".to_string(),
            Scope::Project(id) => format!("scope_type = 'project' AND scope_id = '{}'", id),
            Scope::Session(id) => format!("scope_type = 'session' AND scope_id = '{}'", id),
            Scope::Task(id) => format!("scope_type = 'task' AND scope_id = '{}'", id),
        };

        let count_before = table.count_rows(None).await?;
        table.delete(&filter).await?;
        let count_after = table.count_rows(None).await?;

        Ok(count_before - count_after)
    }

    pub async fn consolidate(&self, from_scope: &Scope, to_scope: &Scope) -> Result<usize> {
        if from_scope.priority() <= to_scope.priority() {
            return Err(MemoryError::InvalidScope(
                "Can only consolidate from higher to lower priority scope".to_string(),
            ));
        }

        let results = self.search_in_scope("", 1000, Some(from_scope)).await?;
        let mut count = 0;

        for result in results {
            if result.entry.importance >= 0.5 {
                self.store_with_importance(
                    &result.entry.content,
                    to_scope.clone(),
                    result.entry.importance * 0.8,
                    result
                        .entry
                        .metadata
                        .as_ref()
                        .and_then(|m| serde_json::from_str(m).ok()),
                )
                .await?;
                count += 1;
            }
        }

        self.forget_scope(from_scope).await?;
        Ok(count)
    }

    pub async fn decay_importance(&self, factor: f32) -> Result<usize> {
        let table = self.get_table().await?;
        let results = table
            .query()
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let entries = parse_record_batches(results)?;
        let mut updated = 0;

        for entry in entries {
            let new_importance = entry.importance * factor;
            if new_importance < 0.1 {
                self.delete(&entry.id).await?;
            }
            updated += 1;
        }

        Ok(updated)
    }

    pub async fn count(&self) -> Result<usize> {
        let table = self.get_table().await?;
        Ok(table.count_rows(None).await?)
    }

    pub async fn list_all(&self) -> Result<Vec<MemoryEntry>> {
        let table = self.get_table().await?;
        let results = table
            .query()
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        parse_record_batches(results)
    }

    pub fn temperature(
        &self,
        entry: &MemoryEntry,
        config: &TemperatureConfig,
    ) -> MemoryTemperature {
        MemoryTemperature::classify(entry.access_count, entry.accessed_at, config)
    }

    pub async fn search_with_metadata_filter(
        &self,
        query_text: &str,
        limit: usize,
        scope: Option<&Scope>,
        metadata_query: &MetadataQuery,
    ) -> Result<Vec<SearchResult>> {
        let over_fetch = limit * 3;
        let results = self.search_in_scope(query_text, over_fetch, scope).await?;

        let mut filtered = Vec::with_capacity(limit);
        for result in results {
            if metadata_query.matches(&result.entry)? {
                filtered.push(result);
                if filtered.len() >= limit {
                    break;
                }
            }
        }
        Ok(filtered)
    }

    pub async fn evict(&self, policy: &dyn EvictionPolicy) -> Result<usize> {
        let entries = self.list_all().await?;
        let ids_to_evict = policy.select_for_eviction(&entries);
        let count = ids_to_evict.len();

        for id in ids_to_evict {
            self.delete(id).await?;
        }

        Ok(count)
    }
}

fn memory_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                EMBEDDING_DIM as i32,
            ),
            false,
        ),
        Field::new("scope_type", DataType::Utf8, false),
        Field::new("scope_id", DataType::Utf8, true),
        Field::new("importance", DataType::Float32, false),
        Field::new("created_at", DataType::Utf8, false),
        Field::new("accessed_at", DataType::Utf8, false),
        Field::new("access_count", DataType::Int32, false),
        Field::new("metadata", DataType::Utf8, true),
    ]))
}

fn create_record_batch(
    id: &str,
    content: &str,
    embedding: &[f32],
    scope: &Scope,
    importance: f32,
    timestamp: DateTime<Utc>,
    metadata: Option<serde_json::Value>,
) -> Result<RecordBatch> {
    let schema = memory_schema();
    let ts_str = timestamp.to_rfc3339();
    let metadata_str = metadata.map(|m| m.to_string());

    let id_array = Arc::new(StringArray::from(vec![id]));
    let content_array = Arc::new(StringArray::from(vec![content]));
    let vector_array = Arc::new(
        FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(embedding.iter().map(|&v| Some(v)).collect::<Vec<_>>())],
            EMBEDDING_DIM as i32,
        ),
    );
    let scope_type_array = Arc::new(StringArray::from(vec![scope.as_str()]));
    let scope_id_array = Arc::new(StringArray::from(vec![scope.id()]));
    let importance_array = Arc::new(Float32Array::from(vec![importance]));
    let created_array = Arc::new(StringArray::from(vec![ts_str.as_str()]));
    let accessed_array = Arc::new(StringArray::from(vec![ts_str.as_str()]));
    let access_count_array = Arc::new(arrow_array::Int32Array::from(vec![0]));
    let metadata_array = Arc::new(StringArray::from(vec![metadata_str.as_deref()]));

    RecordBatch::try_new(
        schema,
        vec![
            id_array,
            content_array,
            vector_array,
            scope_type_array,
            scope_id_array,
            importance_array,
            created_array,
            accessed_array,
            access_count_array,
            metadata_array,
        ],
    )
    .map_err(Into::into)
}

fn parse_record_batches(batches: Vec<RecordBatch>) -> Result<Vec<MemoryEntry>> {
    let mut entries = Vec::new();

    for batch in batches {
        let id_col = batch
            .column_by_name("id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let content_col = batch
            .column_by_name("content")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let scope_type_col = batch
            .column_by_name("scope_type")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let scope_id_col = batch
            .column_by_name("scope_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let importance_col = batch
            .column_by_name("importance")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>());
        let created_col = batch
            .column_by_name("created_at")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let accessed_col = batch
            .column_by_name("accessed_at")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let access_count_col = batch
            .column_by_name("access_count")
            .and_then(|c| c.as_any().downcast_ref::<arrow_array::Int32Array>());
        let metadata_col = batch
            .column_by_name("metadata")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());

        if let (
            Some(ids),
            Some(contents),
            Some(scope_types),
            Some(scope_ids),
            Some(importances),
            Some(createds),
            Some(accesseds),
            Some(access_counts),
            Some(metadatas),
        ) = (
            id_col,
            content_col,
            scope_type_col,
            scope_id_col,
            importance_col,
            created_col,
            accessed_col,
            access_count_col,
            metadata_col,
        ) {
            for i in 0..batch.num_rows() {
                let created_at = createds
                    .value(i)
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now());
                let accessed_at = accesseds
                    .value(i)
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now());

                entries.push(MemoryEntry {
                    id: ids.value(i).to_string(),
                    content: contents.value(i).to_string(),
                    scope_type: scope_types.value(i).to_string(),
                    scope_id: if scope_ids.is_null(i) {
                        None
                    } else {
                        Some(scope_ids.value(i).to_string())
                    },
                    importance: importances.value(i),
                    created_at,
                    accessed_at,
                    access_count: access_counts.value(i),
                    metadata: if metadatas.is_null(i) {
                        None
                    } else {
                        Some(metadatas.value(i).to_string())
                    },
                });
            }
        }
    }

    Ok(entries)
}

fn parse_search_results(batches: Vec<RecordBatch>) -> Result<Vec<SearchResult>> {
    let mut results = Vec::new();

    for batch in batches {
        let entries = parse_record_batches(vec![batch.clone()])?;
        let distance_col = batch
            .column_by_name("_distance")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

        for (i, entry) in entries.into_iter().enumerate() {
            let distance = distance_col.map(|d| d.value(i)).unwrap_or(0.0);
            results.push(SearchResult { entry, distance });
        }
    }

    Ok(results)
}

fn dirs_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("colony")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::open(dir.path()).await.unwrap();

        let id = store
            .store("Test memory content", Scope::Global, None)
            .await
            .unwrap();

        let entry = store.get(&id).await.unwrap().unwrap();
        assert_eq!(entry.content, "Test memory content");
        assert_eq!(entry.scope_type, "global");
    }

    #[tokio::test]
    async fn test_semantic_search() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::open(dir.path()).await.unwrap();

        store
            .store("The user prefers dark mode themes", Scope::Global, None)
            .await
            .unwrap();
        store
            .store("Python is a programming language", Scope::Global, None)
            .await
            .unwrap();
        store
            .store("The user likes verbose error messages", Scope::Global, None)
            .await
            .unwrap();

        let results = store.search("user interface preferences", 2).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_scope_filtering() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::open(dir.path()).await.unwrap();

        store
            .store("Global memory", Scope::Global, None)
            .await
            .unwrap();
        store
            .store(
                "Project memory",
                Scope::Project("test-project".to_string()),
                None,
            )
            .await
            .unwrap();

        let global_results = store
            .search_in_scope("memory", 10, Some(&Scope::Global))
            .await
            .unwrap();
        let project_results = store
            .search_in_scope(
                "memory",
                10,
                Some(&Scope::Project("test-project".to_string())),
            )
            .await
            .unwrap();

        assert_eq!(global_results.len(), 1);
        assert_eq!(project_results.len(), 1);
    }

    #[tokio::test]
    async fn test_delete() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::open(dir.path()).await.unwrap();

        let id = store
            .store("To be deleted", Scope::Global, None)
            .await
            .unwrap();

        assert!(store.get(&id).await.unwrap().is_some());
        store.delete(&id).await.unwrap();
        assert!(store.get(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_forget_scope() {
        let dir = tempdir().unwrap();
        let store = MemoryStore::open(dir.path()).await.unwrap();

        let session = Scope::Session("test-session".to_string());
        store
            .store("Session memory 1", session.clone(), None)
            .await
            .unwrap();
        store
            .store("Session memory 2", session.clone(), None)
            .await
            .unwrap();

        let deleted = store.forget_scope(&session).await.unwrap();
        assert_eq!(deleted, 2);
    }
}
