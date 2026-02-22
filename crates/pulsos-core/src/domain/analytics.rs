use serde::{Deserialize, Serialize};
use std::time::Duration;

mod opt_duration_secs {
    use serde::{Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            None => s.serialize_none(),
            Some(dur) => s.serialize_some(&dur.as_secs_f64()),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        use serde::Deserialize;
        let secs: Option<f64> = Option::deserialize(d)?;
        Ok(secs.map(Duration::from_secs_f64))
    }
}

/// The four DORA DevOps metrics, computed over a window of correlated events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoraMetrics {
    /// Count of successful production deployments in the tracked window.
    pub deployment_frequency: u32,
    /// Mean time from GitHub CI start → CD completion. None if no paired events.
    #[serde(with = "opt_duration_secs")]
    pub lead_time_for_changes: Option<Duration>,
    /// Fraction of deployments where the CD platform ended in failure (0.0–100.0).
    pub change_failure_rate: f64,
    /// Mean time from a failed deployment to the next successful one. None if no failures.
    #[serde(with = "opt_duration_secs")]
    pub time_to_restore_service: Option<Duration>,
    /// The span covered by the history buffer (oldest → newest event).
    #[serde(with = "opt_duration_secs")]
    pub window_duration: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoraRating {
    Elite,
    High,
    Medium,
    Low,
}

impl DoraRating {
    pub fn label(self) -> &'static str {
        match self {
            Self::Elite => "Elite",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

impl DoraMetrics {
    /// DORA rating for Lead Time (< 1h → Elite, < 1d → High, < 1wk → Medium, else Low).
    pub fn lead_time_rating(&self) -> Option<DoraRating> {
        let secs = self.lead_time_for_changes?.as_secs();
        Some(if secs < 3_600 {
            DoraRating::Elite
        } else if secs < 86_400 {
            DoraRating::High
        } else if secs < 604_800 {
            DoraRating::Medium
        } else {
            DoraRating::Low
        })
    }

    /// DORA rating for Change Failure Rate (< 5% → Elite, < 10% → High, < 15% → Medium, else Low).
    pub fn cfr_rating(&self) -> DoraRating {
        if self.change_failure_rate < 5.0 {
            DoraRating::Elite
        } else if self.change_failure_rate < 10.0 {
            DoraRating::High
        } else if self.change_failure_rate < 15.0 {
            DoraRating::Medium
        } else {
            DoraRating::Low
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_time_rating_elite() {
        let m = DoraMetrics {
            lead_time_for_changes: Some(Duration::from_secs(1800)), // 30min
            ..Default::default()
        };
        assert_eq!(m.lead_time_rating(), Some(DoraRating::Elite));
    }

    #[test]
    fn lead_time_rating_high() {
        let m = DoraMetrics {
            lead_time_for_changes: Some(Duration::from_secs(7200)), // 2h
            ..Default::default()
        };
        assert_eq!(m.lead_time_rating(), Some(DoraRating::High));
    }

    #[test]
    fn lead_time_rating_medium() {
        let m = DoraMetrics {
            lead_time_for_changes: Some(Duration::from_secs(172800)), // 2d
            ..Default::default()
        };
        assert_eq!(m.lead_time_rating(), Some(DoraRating::Medium));
    }

    #[test]
    fn lead_time_rating_low() {
        let m = DoraMetrics {
            lead_time_for_changes: Some(Duration::from_secs(1_000_000)), // >1wk
            ..Default::default()
        };
        assert_eq!(m.lead_time_rating(), Some(DoraRating::Low));
    }

    #[test]
    fn lead_time_rating_none_when_no_data() {
        let m = DoraMetrics::default();
        assert_eq!(m.lead_time_rating(), None);
    }

    #[test]
    fn cfr_rating_elite() {
        let m = DoraMetrics {
            change_failure_rate: 3.0,
            ..Default::default()
        };
        assert_eq!(m.cfr_rating(), DoraRating::Elite);
    }

    #[test]
    fn cfr_rating_high() {
        let m = DoraMetrics {
            change_failure_rate: 7.0,
            ..Default::default()
        };
        assert_eq!(m.cfr_rating(), DoraRating::High);
    }

    #[test]
    fn cfr_rating_medium() {
        let m = DoraMetrics {
            change_failure_rate: 12.0,
            ..Default::default()
        };
        assert_eq!(m.cfr_rating(), DoraRating::Medium);
    }

    #[test]
    fn cfr_rating_low() {
        let m = DoraMetrics {
            change_failure_rate: 20.0,
            ..Default::default()
        };
        assert_eq!(m.cfr_rating(), DoraRating::Low);
    }
}
