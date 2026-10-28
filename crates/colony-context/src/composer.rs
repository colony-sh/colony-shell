//! Composes a unified context from multiple sources within a token budget.

use crate::entry::{ContextEntry, ContextKind};
use crate::project::ProjectContext;
use crate::store::ContextStore;

/// Builds a unified `@context` string for agent consumption.
pub struct ContextComposer {
    max_tokens: usize,
}

impl ContextComposer {
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }

    /// Compose context for a project, respecting the token budget.
    ///
    /// Priority order:
    /// 1. Project context (auto-gathered)
    /// 2. Pinned entries (always included)
    /// 3. Channel entries (fd 3)
    /// 4. Remaining entries by priority (descending), then use count (descending)
    pub fn compose(
        &self,
        store: &ContextStore,
        project_id: &str,
        project_ctx: &ProjectContext,
        channel_entries: &[String],
    ) -> ComposedContext {
        let file = store.load(project_id).unwrap_or_default();
        let mut sections: Vec<ContextSection> = Vec::new();
        let mut used_tokens = 0;

        // 1. Project context (auto-gathered)
        let project_text = project_ctx.render();
        let project_tokens = project_text.len() / 4;
        if used_tokens + project_tokens <= self.max_tokens {
            sections.push(ContextSection {
                label: "Project".to_string(),
                content: project_text,
                tokens: project_tokens,
                kind: ContextKind::Project,
            });
            used_tokens += project_tokens;
        }

        // 2. Pinned entries (always included)
        for entry in file.entries.iter().filter(|e| e.metadata.pinned) {
            if used_tokens + entry.metadata.tokens > self.max_tokens {
                break;
            }
            sections.push(ContextSection::from_entry(entry));
            used_tokens += entry.metadata.tokens;
        }

        // 3. Channel entries (fd 3 -- most recent first)
        for line in channel_entries.iter().rev().take(20) {
            let tokens = line.len() / 4;
            if used_tokens + tokens > self.max_tokens {
                break;
            }
            sections.push(ContextSection {
                label: "Channel".to_string(),
                content: line.clone(),
                tokens,
                kind: ContextKind::Channel,
            });
            used_tokens += tokens;
        }

        // 4. Remaining entries by priority (descending), then use_count (descending)
        let mut remaining: Vec<&ContextEntry> =
            file.entries.iter().filter(|e| !e.metadata.pinned).collect();
        remaining.sort_by(|a, b| {
            b.metadata
                .priority
                .cmp(&a.metadata.priority)
                .then_with(|| b.metadata.use_count.cmp(&a.metadata.use_count))
        });

        for entry in remaining {
            if used_tokens + entry.metadata.tokens > self.max_tokens {
                continue; // Skip large entries, try smaller ones
            }
            sections.push(ContextSection::from_entry(entry));
            used_tokens += entry.metadata.tokens;
        }

        ComposedContext {
            sections,
            total_tokens: used_tokens,
            budget: self.max_tokens,
        }
    }
}

/// A section in the composed context.
#[derive(Debug, Clone)]
pub struct ContextSection {
    pub label: String,
    pub content: String,
    pub tokens: usize,
    pub kind: ContextKind,
}

impl ContextSection {
    fn from_entry(entry: &ContextEntry) -> Self {
        Self {
            label: entry
                .label
                .clone()
                .unwrap_or_else(|| format!("{:?}", entry.kind)),
            content: entry.content.clone(),
            tokens: entry.metadata.tokens,
            kind: entry.kind,
        }
    }
}

/// Result of context composition.
#[derive(Debug, Clone)]
pub struct ComposedContext {
    pub sections: Vec<ContextSection>,
    pub total_tokens: usize,
    pub budget: usize,
}

impl ComposedContext {
    /// Render as a single string for injection into agent context.
    pub fn render(&self) -> String {
        self.sections
            .iter()
            .map(|s| format!("## {}\n{}", s.label, s.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Token utilization as a fraction (0.0-1.0).
    pub fn utilization(&self) -> f64 {
        if self.budget == 0 {
            return 0.0;
        }
        self.total_tokens as f64 / self.budget as f64
    }
}

impl Default for ComposedContext {
    fn default() -> Self {
        Self {
            sections: Vec::new(),
            total_tokens: 0,
            budget: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::ContextEntry;
    use tempfile::TempDir;

    #[test]
    fn compose_includes_project() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());
        let project = ProjectContext {
            root: "/test".to_string(),
            git_branch: Some("main".to_string()),
            git_status_summary: None,
            languages: vec!["Rust".to_string()],
            build_file: Some("Cargo.toml".to_string()),
        };

        let composer = ContextComposer::new(10000);
        let result = composer.compose(&store, "test", &project, &[]);
        assert!(!result.sections.is_empty());
        assert!(result.render().contains("Rust"));
    }

    #[test]
    fn compose_respects_budget() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        // Add a large entry
        let big = ContextEntry::new(ContextKind::File, "x".repeat(40000));
        store.add_entry("proj", big).unwrap();

        let project = ProjectContext {
            root: "/test".to_string(),
            git_branch: None,
            git_status_summary: None,
            languages: Vec::new(),
            build_file: None,
        };

        let composer = ContextComposer::new(100); // Very small budget
        let result = composer.compose(&store, "proj", &project, &[]);
        assert!(result.total_tokens <= 100);
    }

    #[test]
    fn compose_includes_pinned_first() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());

        let pinned = ContextEntry::new(ContextKind::Knowledge, "important".to_string()).pinned();
        let normal = ContextEntry::new(ContextKind::Knowledge, "meh".to_string());
        store.add_entry("proj", normal).unwrap();
        store.add_entry("proj", pinned).unwrap();

        let project = ProjectContext {
            root: "/test".to_string(),
            git_branch: None,
            git_status_summary: None,
            languages: Vec::new(),
            build_file: None,
        };

        let composer = ContextComposer::new(10000);
        let result = composer.compose(&store, "proj", &project, &[]);
        let rendered = result.render();
        let pinned_pos = rendered.find("important").unwrap();
        let normal_pos = rendered.find("meh").unwrap();
        assert!(pinned_pos < normal_pos);
    }

    #[test]
    fn compose_includes_channel_entries() {
        let tmp = TempDir::new().unwrap();
        let store = ContextStore::new(tmp.path().to_path_buf());
        let project = ProjectContext {
            root: "/test".to_string(),
            git_branch: None,
            git_status_summary: None,
            languages: Vec::new(),
            build_file: None,
        };

        let channels = vec!["channel line 1".to_string(), "channel line 2".to_string()];
        let composer = ContextComposer::new(10000);
        let result = composer.compose(&store, "proj", &project, &channels);
        let rendered = result.render();
        assert!(rendered.contains("channel line 1"));
        assert!(rendered.contains("channel line 2"));
    }

    #[test]
    fn utilization_zero_budget() {
        let ctx = ComposedContext::default();
        assert_eq!(ctx.utilization(), 0.0);
    }

    #[test]
    fn utilization_half() {
        let ctx = ComposedContext {
            sections: Vec::new(),
            total_tokens: 500,
            budget: 1000,
        };
        assert!((ctx.utilization() - 0.5).abs() < f64::EPSILON);
    }
}
