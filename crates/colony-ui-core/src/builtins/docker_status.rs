use crate::component::{ComponentOutput, PromptComponent, TuplePattern};
use crate::context::ComponentContext;
use crate::{Color, Style, StyledSpan};
use parking_lot::Mutex;
use std::process::Command;
use std::time::{Duration, Instant};

/// How long the cached container count is considered fresh.
const CACHE_TTL: Duration = Duration::from_secs(10);

struct CachedCount {
    value: Option<usize>,
    fetched_at: Instant,
}

pub struct DockerStatusComponent {
    cache: Mutex<Option<CachedCount>>,
}

impl DockerStatusComponent {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(None),
        }
    }

    fn get_running_count(&self) -> Option<usize> {
        let mut cache = self.cache.lock();

        if let Some(ref cached) = *cache {
            if cached.fetched_at.elapsed() < CACHE_TTL {
                return cached.value;
            }
        }

        let value = Self::fetch_running_count();
        *cache = Some(CachedCount {
            value,
            fetched_at: Instant::now(),
        });
        value
    }

    fn fetch_running_count() -> Option<usize> {
        let output = Command::new("docker").args(["ps", "-q"]).output().ok()?;

        if output.status.success() {
            let count = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count();
            Some(count)
        } else {
            None
        }
    }
}

impl Default for DockerStatusComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptComponent for DockerStatusComponent {
    fn id(&self) -> &str {
        "docker_status"
    }

    fn subscriptions(&self) -> Vec<TuplePattern> {
        vec![]
    }

    fn render(&self, _ctx: &ComponentContext) -> ComponentOutput {
        match self.get_running_count() {
            Some(count) if count > 0 => {
                let text = format!("🐳 {}", count);
                let spans = vec![StyledSpan::new(text, Style::default().fg(Color::Cyan))];
                ComponentOutput::with_data(spans, serde_json::json!({ "running": count }))
            }
            _ => ComponentOutput::hidden(),
        }
    }
}
