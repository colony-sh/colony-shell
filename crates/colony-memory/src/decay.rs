use crate::store::MemoryStore;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct DecayConfig {
    pub decay_rate: f32,
    pub interval: std::time::Duration,
    pub eviction_threshold: f32,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.95,
            interval: std::time::Duration::from_secs(3600),
            eviction_threshold: 0.1,
        }
    }
}

pub struct ImportanceDecayScheduler {
    config: DecayConfig,
    shutdown_tx: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl ImportanceDecayScheduler {
    pub fn new(config: DecayConfig) -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            config,
            shutdown_tx,
            handle: None,
        }
    }

    pub fn start(&mut self, store: Arc<MemoryStore>) {
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.interval);
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match store.decay_importance(config.decay_rate).await {
                            Ok(updated) => {
                                info!(
                                    "Decay cycle complete: {} memories processed (rate={})",
                                    updated, config.decay_rate
                                );
                            }
                            Err(e) => {
                                warn!("Decay cycle failed: {}", e);
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        info!("Decay scheduler shutting down");
                        break;
                    }
                }
            }
        });

        self.handle = Some(handle);
    }

    pub fn stop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        self.handle = None;
    }

    pub fn is_running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    pub fn config(&self) -> &DecayConfig {
        &self.config
    }
}

impl Drop for ImportanceDecayScheduler {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decay_config_defaults() {
        let config = DecayConfig::default();
        assert!((config.decay_rate - 0.95).abs() < f32::EPSILON);
        assert_eq!(config.interval, std::time::Duration::from_secs(3600));
        assert!((config.eviction_threshold - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn test_scheduler_not_running_before_start() {
        let scheduler = ImportanceDecayScheduler::new(DecayConfig::default());
        assert!(!scheduler.is_running());
    }

    #[tokio::test]
    async fn test_scheduler_start_and_stop() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());

        let config = DecayConfig {
            decay_rate: 0.9,
            interval: std::time::Duration::from_millis(50),
            eviction_threshold: 0.1,
        };
        let mut scheduler = ImportanceDecayScheduler::new(config);
        scheduler.start(store);

        assert!(scheduler.is_running());

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        scheduler.stop();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!scheduler.is_running());
    }

    #[tokio::test]
    async fn test_scheduler_drop_stops() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(MemoryStore::open(dir.path()).await.unwrap());

        let config = DecayConfig {
            decay_rate: 0.9,
            interval: std::time::Duration::from_millis(50),
            eviction_threshold: 0.1,
        };
        let mut scheduler = ImportanceDecayScheduler::new(config);
        scheduler.start(store);

        drop(scheduler);
    }

    #[test]
    fn test_config_accessor() {
        let config = DecayConfig {
            decay_rate: 0.8,
            interval: std::time::Duration::from_secs(60),
            eviction_threshold: 0.2,
        };
        let scheduler = ImportanceDecayScheduler::new(config);
        assert!((scheduler.config().decay_rate - 0.8).abs() < f32::EPSILON);
    }
}
