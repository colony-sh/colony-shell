use crate::store::MemoryEntry;
use chrono::{Duration, Utc};

pub trait EvictionPolicy: Send + Sync {
    fn select_for_eviction<'a>(&self, entries: &'a [MemoryEntry]) -> Vec<&'a str>;
}

pub struct LruEvictionPolicy {
    pub max_entries: usize,
}

impl LruEvictionPolicy {
    pub fn new(max_entries: usize) -> Self {
        Self { max_entries }
    }
}

impl EvictionPolicy for LruEvictionPolicy {
    fn select_for_eviction<'a>(&self, entries: &'a [MemoryEntry]) -> Vec<&'a str> {
        if entries.len() <= self.max_entries {
            return Vec::new();
        }

        let excess = entries.len() - self.max_entries;
        let mut sorted: Vec<&MemoryEntry> = entries.iter().collect();
        sorted.sort_by(|a, b| a.accessed_at.cmp(&b.accessed_at));

        sorted.iter().take(excess).map(|e| e.id.as_str()).collect()
    }
}

pub struct TtlEvictionPolicy {
    pub max_age: Duration,
}

impl TtlEvictionPolicy {
    pub fn new(max_age: Duration) -> Self {
        Self { max_age }
    }
}

impl EvictionPolicy for TtlEvictionPolicy {
    fn select_for_eviction<'a>(&self, entries: &'a [MemoryEntry]) -> Vec<&'a str> {
        let cutoff = Utc::now() - self.max_age;
        entries
            .iter()
            .filter(|e| e.created_at < cutoff)
            .map(|e| e.id.as_str())
            .collect()
    }
}

pub struct CompositeEvictionPolicy {
    policies: Vec<Box<dyn EvictionPolicy>>,
}

impl CompositeEvictionPolicy {
    pub fn new(policies: Vec<Box<dyn EvictionPolicy>>) -> Self {
        Self { policies }
    }
}

impl EvictionPolicy for CompositeEvictionPolicy {
    fn select_for_eviction<'a>(&self, entries: &'a [MemoryEntry]) -> Vec<&'a str> {
        let mut ids = std::collections::HashSet::new();
        for policy in &self.policies {
            for id in policy.select_for_eviction(entries) {
                ids.insert(id);
            }
        }
        ids.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_entry(id: &str, accessed_hours_ago: i64, created_hours_ago: i64) -> MemoryEntry {
        let now = Utc::now();
        MemoryEntry {
            id: id.to_string(),
            content: format!("content for {}", id),
            scope_type: "global".to_string(),
            scope_id: None,
            importance: 1.0,
            created_at: now - Duration::hours(created_hours_ago),
            accessed_at: now - Duration::hours(accessed_hours_ago),
            access_count: 0,
            metadata: None,
        }
    }

    #[test]
    fn test_lru_no_eviction_under_capacity() {
        let policy = LruEvictionPolicy::new(5);
        let entries = vec![make_entry("a", 1, 1), make_entry("b", 2, 2)];
        let evicted = policy.select_for_eviction(&entries);
        assert!(evicted.is_empty());
    }

    #[test]
    fn test_lru_evicts_least_recently_accessed() {
        let policy = LruEvictionPolicy::new(2);
        let entries = vec![
            make_entry("a", 10, 10),
            make_entry("b", 1, 5),
            make_entry("c", 5, 3),
        ];
        let evicted = policy.select_for_eviction(&entries);
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], "a");
    }

    #[test]
    fn test_lru_evicts_multiple() {
        let policy = LruEvictionPolicy::new(1);
        let entries = vec![
            make_entry("a", 10, 10),
            make_entry("b", 5, 5),
            make_entry("c", 1, 1),
        ];
        let evicted = policy.select_for_eviction(&entries);
        assert_eq!(evicted.len(), 2);
        assert!(evicted.contains(&"a"));
        assert!(evicted.contains(&"b"));
    }

    #[test]
    fn test_ttl_no_eviction_within_window() {
        let policy = TtlEvictionPolicy::new(Duration::hours(24));
        let entries = vec![make_entry("a", 1, 1), make_entry("b", 2, 2)];
        let evicted = policy.select_for_eviction(&entries);
        assert!(evicted.is_empty());
    }

    #[test]
    fn test_ttl_evicts_old_entries() {
        let policy = TtlEvictionPolicy::new(Duration::hours(24));
        let entries = vec![
            make_entry("a", 1, 1),
            make_entry("b", 2, 48),
            make_entry("c", 3, 72),
        ];
        let evicted = policy.select_for_eviction(&entries);
        assert_eq!(evicted.len(), 2);
        assert!(evicted.contains(&"b"));
        assert!(evicted.contains(&"c"));
    }

    #[test]
    fn test_composite_combines_policies() {
        let lru = Box::new(LruEvictionPolicy::new(2));
        let ttl = Box::new(TtlEvictionPolicy::new(Duration::hours(24)));
        let composite = CompositeEvictionPolicy::new(vec![lru, ttl]);

        let entries = vec![
            make_entry("a", 10, 48),
            make_entry("b", 5, 5),
            make_entry("c", 1, 1),
        ];

        let evicted = composite.select_for_eviction(&entries);
        assert!(evicted.contains(&"a"));
    }

    #[test]
    fn test_composite_deduplicates() {
        let lru = Box::new(LruEvictionPolicy::new(2));
        let ttl = Box::new(TtlEvictionPolicy::new(Duration::hours(6)));
        let composite = CompositeEvictionPolicy::new(vec![lru, ttl]);

        let entries = vec![
            make_entry("a", 10, 10),
            make_entry("b", 5, 5),
            make_entry("c", 1, 1),
        ];

        let evicted = composite.select_for_eviction(&entries);
        let unique: std::collections::HashSet<_> = evicted.iter().collect();
        assert_eq!(evicted.len(), unique.len());
    }
}
