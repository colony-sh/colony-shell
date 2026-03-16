//! Search query builder for history queries.

/// A structured search query with optional filters.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// FTS5 match text.
    pub text: String,
    /// Optional directory filter.
    pub cwd: Option<String>,
    /// Optional executor filter.
    pub executor: Option<String>,
    /// Maximum number of results.
    pub limit: u32,
    /// Whether to deduplicate by command text.
    pub dedup: bool,
}

impl SearchQuery {
    /// Create a new search query with the given text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            cwd: None,
            executor: None,
            limit: 50,
            dedup: false,
        }
    }

    /// Filter results to a specific working directory.
    pub fn cwd_filter(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Filter results to a specific executor.
    pub fn executor_filter(mut self, executor: impl Into<String>) -> Self {
        self.executor = Some(executor.into());
        self
    }

    /// Set the maximum number of results to return.
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Enable or disable deduplication by command text.
    pub fn dedup(mut self, dedup: bool) -> Self {
        self.dedup = dedup;
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_query_builder() {
        let query = SearchQuery::new("cargo build")
            .cwd_filter("/project")
            .executor_filter("user")
            .limit(20)
            .dedup(true);

        assert_eq!(query.text, "cargo build");
        assert_eq!(query.cwd, Some("/project".to_string()));
        assert_eq!(query.executor, Some("user".to_string()));
        assert_eq!(query.limit, 20);
        assert!(query.dedup);
    }

    #[test]
    fn test_search_query_defaults() {
        let query = SearchQuery::new("test");
        assert_eq!(query.limit, 50);
        assert!(!query.dedup);
        assert!(query.cwd.is_none());
        assert!(query.executor.is_none());
    }
}
