use crate::error::{MemoryError, Result};
use crate::store::MemoryEntry;

#[derive(Debug, Clone)]
pub enum MetadataOperator {
    Equals(serde_json::Value),
    Contains(String),
    Range { min: Option<f64>, max: Option<f64> },
    Exists,
}

#[derive(Debug, Clone)]
pub struct MetadataCondition {
    pub field: String,
    pub operator: MetadataOperator,
}

#[derive(Debug, Clone)]
pub enum MetadataQuery {
    Condition(MetadataCondition),
    And(Vec<MetadataQuery>),
    Or(Vec<MetadataQuery>),
}

impl MetadataQuery {
    pub fn equals(field: impl Into<String>, value: serde_json::Value) -> Self {
        Self::Condition(MetadataCondition {
            field: field.into(),
            operator: MetadataOperator::Equals(value),
        })
    }

    pub fn contains(field: impl Into<String>, substring: impl Into<String>) -> Self {
        Self::Condition(MetadataCondition {
            field: field.into(),
            operator: MetadataOperator::Contains(substring.into()),
        })
    }

    pub fn range(field: impl Into<String>, min: Option<f64>, max: Option<f64>) -> Self {
        Self::Condition(MetadataCondition {
            field: field.into(),
            operator: MetadataOperator::Range { min, max },
        })
    }

    pub fn exists(field: impl Into<String>) -> Self {
        Self::Condition(MetadataCondition {
            field: field.into(),
            operator: MetadataOperator::Exists,
        })
    }

    pub fn and(queries: Vec<MetadataQuery>) -> Self {
        Self::And(queries)
    }

    pub fn or(queries: Vec<MetadataQuery>) -> Self {
        Self::Or(queries)
    }

    pub fn matches(&self, entry: &MemoryEntry) -> Result<bool> {
        let metadata: serde_json::Value = match &entry.metadata {
            Some(m) => serde_json::from_str(m)?,
            None => return Ok(false),
        };

        self.matches_value(&metadata)
    }

    fn matches_value(&self, metadata: &serde_json::Value) -> Result<bool> {
        match self {
            Self::Condition(condition) => evaluate_condition(condition, metadata),
            Self::And(queries) => {
                for q in queries {
                    if !q.matches_value(metadata)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Or(queries) => {
                for q in queries {
                    if q.matches_value(metadata)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}

fn evaluate_condition(condition: &MetadataCondition, metadata: &serde_json::Value) -> Result<bool> {
    let field_value = resolve_field(metadata, &condition.field);

    match &condition.operator {
        MetadataOperator::Equals(expected) => match field_value {
            Some(actual) => Ok(actual == expected),
            None => Ok(false),
        },
        MetadataOperator::Contains(substring) => match field_value {
            Some(serde_json::Value::String(s)) => Ok(s.contains(substring.as_str())),
            Some(serde_json::Value::Array(arr)) => {
                let needle = serde_json::Value::String(substring.clone());
                Ok(arr.contains(&needle))
            }
            _ => Ok(false),
        },
        MetadataOperator::Range { min, max } => match field_value {
            Some(v) => {
                let num = v.as_f64().ok_or_else(|| {
                    MemoryError::InvalidScope(format!(
                        "field '{}' is not a number for range comparison",
                        condition.field
                    ))
                })?;
                let above_min = min.map_or(true, |m| num >= m);
                let below_max = max.map_or(true, |m| num <= m);
                Ok(above_min && below_max)
            }
            None => Ok(false),
        },
        MetadataOperator::Exists => Ok(field_value.is_some()),
    }
}

fn resolve_field<'a>(value: &'a serde_json::Value, field: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for part in field.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

pub fn filter_entries(
    entries: Vec<MemoryEntry>,
    query: &MetadataQuery,
) -> Result<Vec<MemoryEntry>> {
    let mut filtered = Vec::with_capacity(entries.len());
    for entry in entries {
        if query.matches(&entry)? {
            filtered.push(entry);
        }
    }
    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn make_entry(metadata: serde_json::Value) -> MemoryEntry {
        MemoryEntry {
            id: "test-id".to_string(),
            content: "test content".to_string(),
            scope_type: "global".to_string(),
            scope_id: None,
            importance: 1.0,
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            access_count: 0,
            metadata: Some(metadata.to_string()),
        }
    }

    #[test]
    fn test_equals_string() {
        let entry = make_entry(json!({"role": "user", "type": "question"}));
        let query = MetadataQuery::equals("role", json!("user"));
        assert!(query.matches(&entry).unwrap());

        let query = MetadataQuery::equals("role", json!("assistant"));
        assert!(!query.matches(&entry).unwrap());
    }

    #[test]
    fn test_equals_number() {
        let entry = make_entry(json!({"priority": 5}));
        let query = MetadataQuery::equals("priority", json!(5));
        assert!(query.matches(&entry).unwrap());

        let query = MetadataQuery::equals("priority", json!(3));
        assert!(!query.matches(&entry).unwrap());
    }

    #[test]
    fn test_contains_string() {
        let entry = make_entry(json!({"tags": "rust,programming,shell"}));
        let query = MetadataQuery::contains("tags", "rust");
        assert!(query.matches(&entry).unwrap());

        let query = MetadataQuery::contains("tags", "python");
        assert!(!query.matches(&entry).unwrap());
    }

    #[test]
    fn test_contains_array() {
        let entry = make_entry(json!({"tags": ["rust", "programming", "shell"]}));
        let query = MetadataQuery::contains("tags", "rust");
        assert!(query.matches(&entry).unwrap());

        let query = MetadataQuery::contains("tags", "python");
        assert!(!query.matches(&entry).unwrap());
    }

    #[test]
    fn test_range() {
        let entry = make_entry(json!({"score": 7.5}));

        let query = MetadataQuery::range("score", Some(5.0), Some(10.0));
        assert!(query.matches(&entry).unwrap());

        let query = MetadataQuery::range("score", Some(8.0), None);
        assert!(!query.matches(&entry).unwrap());

        let query = MetadataQuery::range("score", None, Some(8.0));
        assert!(query.matches(&entry).unwrap());
    }

    #[test]
    fn test_exists() {
        let entry = make_entry(json!({"role": "user"}));
        let query = MetadataQuery::exists("role");
        assert!(query.matches(&entry).unwrap());

        let query = MetadataQuery::exists("nonexistent");
        assert!(!query.matches(&entry).unwrap());
    }

    #[test]
    fn test_and_query() {
        let entry = make_entry(json!({"role": "user", "priority": 5}));
        let query = MetadataQuery::and(vec![
            MetadataQuery::equals("role", json!("user")),
            MetadataQuery::equals("priority", json!(5)),
        ]);
        assert!(query.matches(&entry).unwrap());

        let query = MetadataQuery::and(vec![
            MetadataQuery::equals("role", json!("user")),
            MetadataQuery::equals("priority", json!(3)),
        ]);
        assert!(!query.matches(&entry).unwrap());
    }

    #[test]
    fn test_or_query() {
        let entry = make_entry(json!({"role": "user", "priority": 5}));
        let query = MetadataQuery::or(vec![
            MetadataQuery::equals("role", json!("assistant")),
            MetadataQuery::equals("priority", json!(5)),
        ]);
        assert!(query.matches(&entry).unwrap());

        let query = MetadataQuery::or(vec![
            MetadataQuery::equals("role", json!("assistant")),
            MetadataQuery::equals("priority", json!(3)),
        ]);
        assert!(!query.matches(&entry).unwrap());
    }

    #[test]
    fn test_nested_query() {
        let entry = make_entry(json!({"role": "user", "priority": 5, "source": "chat"}));
        let query = MetadataQuery::and(vec![
            MetadataQuery::equals("role", json!("user")),
            MetadataQuery::or(vec![
                MetadataQuery::equals("priority", json!(5)),
                MetadataQuery::equals("source", json!("api")),
            ]),
        ]);
        assert!(query.matches(&entry).unwrap());
    }

    #[test]
    fn test_nested_field_path() {
        let entry = make_entry(json!({"context": {"project": "colony"}}));
        let query = MetadataQuery::equals("context.project", json!("colony"));
        assert!(query.matches(&entry).unwrap());
    }

    #[test]
    fn test_no_metadata() {
        let entry = MemoryEntry {
            id: "test".to_string(),
            content: "test".to_string(),
            scope_type: "global".to_string(),
            scope_id: None,
            importance: 1.0,
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            access_count: 0,
            metadata: None,
        };
        let query = MetadataQuery::equals("role", json!("user"));
        assert!(!query.matches(&entry).unwrap());
    }

    #[test]
    fn test_filter_entries() {
        let entries = vec![
            make_entry(json!({"role": "user"})),
            make_entry(json!({"role": "assistant"})),
            make_entry(json!({"role": "user"})),
        ];
        let query = MetadataQuery::equals("role", json!("user"));
        let filtered = filter_entries(entries, &query).unwrap();
        assert_eq!(filtered.len(), 2);
    }
}
