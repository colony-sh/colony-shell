use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryTemperature {
    Hot,
    Warm,
    Cold,
}

impl MemoryTemperature {
    pub fn classify(access_count: i32, last_accessed: DateTime<Utc>, config: &TemperatureConfig) -> Self {
        let age = Utc::now().signed_duration_since(last_accessed);
        let age_hours = age.num_hours();

        if access_count >= config.hot_access_threshold && age_hours <= config.hot_recency_hours {
            Self::Hot
        } else if access_count >= config.warm_access_threshold && age_hours <= config.warm_recency_hours {
            Self::Warm
        } else {
            Self::Cold
        }
    }
}

impl std::fmt::Display for MemoryTemperature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hot => write!(f, "hot"),
            Self::Warm => write!(f, "warm"),
            Self::Cold => write!(f, "cold"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TemperatureConfig {
    pub hot_access_threshold: i32,
    pub hot_recency_hours: i64,
    pub warm_access_threshold: i32,
    pub warm_recency_hours: i64,
}

impl Default for TemperatureConfig {
    fn default() -> Self {
        Self {
            hot_access_threshold: 5,
            hot_recency_hours: 24,
            warm_access_threshold: 2,
            warm_recency_hours: 72,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_hot_classification() {
        let config = TemperatureConfig::default();
        let now = Utc::now();
        let temp = MemoryTemperature::classify(10, now, &config);
        assert_eq!(temp, MemoryTemperature::Hot);
    }

    #[test]
    fn test_warm_classification() {
        let config = TemperatureConfig::default();
        let two_days_ago = Utc::now() - Duration::hours(48);
        let temp = MemoryTemperature::classify(3, two_days_ago, &config);
        assert_eq!(temp, MemoryTemperature::Warm);
    }

    #[test]
    fn test_cold_classification() {
        let config = TemperatureConfig::default();
        let week_ago = Utc::now() - Duration::hours(168);
        let temp = MemoryTemperature::classify(1, week_ago, &config);
        assert_eq!(temp, MemoryTemperature::Cold);
    }

    #[test]
    fn test_high_access_but_old_is_warm_then_cold() {
        let config = TemperatureConfig::default();
        let three_days_ago = Utc::now() - Duration::hours(48);
        let temp = MemoryTemperature::classify(10, three_days_ago, &config);
        assert_eq!(temp, MemoryTemperature::Warm);

        let old = Utc::now() - Duration::hours(200);
        let temp = MemoryTemperature::classify(10, old, &config);
        assert_eq!(temp, MemoryTemperature::Cold);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", MemoryTemperature::Hot), "hot");
        assert_eq!(format!("{}", MemoryTemperature::Warm), "warm");
        assert_eq!(format!("{}", MemoryTemperature::Cold), "cold");
    }

    #[test]
    fn test_custom_config() {
        let config = TemperatureConfig {
            hot_access_threshold: 3,
            hot_recency_hours: 12,
            warm_access_threshold: 1,
            warm_recency_hours: 48,
        };
        let now = Utc::now();
        let temp = MemoryTemperature::classify(3, now, &config);
        assert_eq!(temp, MemoryTemperature::Hot);
    }
}
