use crate::error::{MemoryError, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::{Arc, Mutex};

pub const EMBEDDING_DIM: usize = 384;

pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}

pub struct FastEmbedProvider {
    model: Arc<Mutex<TextEmbedding>>,
}

impl FastEmbedProvider {
    pub fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(true),
        )
        .map_err(|e| MemoryError::Embedding(e.to_string()))?;

        Ok(Self {
            model: Arc::new(Mutex::new(model)),
        })
    }

    pub fn with_model(model_name: EmbeddingModel) -> Result<Self> {
        let model =
            TextEmbedding::try_new(InitOptions::new(model_name).with_show_download_progress(true))
                .map_err(|e| MemoryError::Embedding(e.to_string()))?;

        Ok(Self {
            model: Arc::new(Mutex::new(model)),
        })
    }
}

impl EmbeddingProvider for FastEmbedProvider {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let model = self
            .model
            .lock()
            .map_err(|e| MemoryError::Embedding(format!("Lock poisoned: {}", e)))?;
        let texts_owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        model
            .embed(texts_owned, None)
            .map_err(|e| MemoryError::Embedding(e.to_string()))
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIM
    }
}

impl Default for FastEmbedProvider {
    fn default() -> Self {
        Self::new().expect("Failed to initialize FastEmbed model")
    }
}

impl Clone for FastEmbedProvider {
    fn clone(&self) -> Self {
        Self {
            model: Arc::clone(&self.model),
        }
    }
}
