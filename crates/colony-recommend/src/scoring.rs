//! Confidence scoring utilities.

use super::source::Recommendation;

/// Score a prefix match (0.0--1.0).
pub fn prefix_score(query: &str, candidate: &str) -> f64 {
    if candidate.starts_with(query) {
        0.8 + (0.2 * query.len() as f64 / candidate.len().max(1) as f64)
    } else {
        0.0
    }
}

/// Score a fuzzy match (0.0--1.0).
pub fn fuzzy_score(query: &str, candidate: &str) -> f64 {
    if query.is_empty() {
        return 0.0;
    }

    let query_lower = query.to_lowercase();
    let candidate_lower = candidate.to_lowercase();

    // Exact prefix is best
    if candidate_lower.starts_with(&query_lower) {
        return prefix_score(query, candidate);
    }

    // Substring match
    if candidate_lower.contains(&query_lower) {
        return 0.5 + (0.2 * query.len() as f64 / candidate.len().max(1) as f64);
    }

    // Character-by-character fuzzy
    let mut query_chars = query_lower.chars().peekable();
    let mut matched = 0;
    for c in candidate_lower.chars() {
        if query_chars.peek() == Some(&c) {
            query_chars.next();
            matched += 1;
        }
    }

    if query_chars.peek().is_some() {
        return 0.0; // Not all chars matched
    }

    0.2 * (matched as f64 / candidate.len().max(1) as f64)
}

/// Sort recommendations by confidence descending, then alphabetically.
pub fn rank(recommendations: &mut [Recommendation]) {
    recommendations.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.text.cmp(&b.text))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_match_scores_high() {
        let score = prefix_score("git", "git");
        assert!(score > 0.9);
    }

    #[test]
    fn prefix_partial_scores_moderate() {
        let score = prefix_score("gi", "git");
        assert!(score > 0.8);
    }

    #[test]
    fn no_match_scores_zero() {
        let score = prefix_score("xyz", "git");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn fuzzy_substring_scores() {
        let score = fuzzy_score("commit", "git-commit");
        assert!(score > 0.4);
    }

    #[test]
    fn fuzzy_empty_query_scores_zero() {
        let score = fuzzy_score("", "anything");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn fuzzy_exact_prefix_scores_high() {
        let score = fuzzy_score("car", "cargo");
        assert!(score > 0.8);
    }

    #[test]
    fn fuzzy_no_match_scores_zero() {
        let score = fuzzy_score("xyz", "abc");
        assert_eq!(score, 0.0);
    }

    #[test]
    fn rank_sorts_by_confidence_desc() {
        use crate::source::RecommendKind;

        let mut recs = vec![
            Recommendation {
                text: "low".to_string(),
                label: "low".to_string(),
                description: None,
                kind: RecommendKind::History,
                confidence: 0.3,
                match_indices: Vec::new(),
            },
            Recommendation {
                text: "high".to_string(),
                label: "high".to_string(),
                description: None,
                kind: RecommendKind::History,
                confidence: 0.9,
                match_indices: Vec::new(),
            },
            Recommendation {
                text: "mid".to_string(),
                label: "mid".to_string(),
                description: None,
                kind: RecommendKind::History,
                confidence: 0.6,
                match_indices: Vec::new(),
            },
        ];

        rank(&mut recs);
        assert_eq!(recs[0].text, "high");
        assert_eq!(recs[1].text, "mid");
        assert_eq!(recs[2].text, "low");
    }
}
