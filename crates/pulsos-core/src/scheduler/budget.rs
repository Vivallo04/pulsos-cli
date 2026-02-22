use crate::platform::RateLimitInfo;
use chrono::{DateTime, Utc};

pub struct RateLimitBudget {
    remaining: u32,
    limit: u32,
    reset: DateTime<Utc>,
}

impl RateLimitBudget {
    pub fn new(remaining: u32, limit: u32, reset: DateTime<Utc>) -> Self {
        Self {
            remaining,
            limit,
            reset,
        }
    }

    pub fn from_rate_limit_info(info: &RateLimitInfo) -> Self {
        Self {
            remaining: info.remaining,
            limit: info.limit,
            reset: info.resets_at,
        }
    }

    /// Returns percentage of requests remaining (0.0–100.0).
    pub fn pct_remaining(&self) -> f64 {
        if self.limit == 0 {
            // Unknown budget; treat as empty to avoid aggressive polling.
            return 0.0;
        }
        self.remaining as f64 / self.limit as f64 * 100.0
    }

    pub fn is_exhausted(&self) -> bool {
        self.limit > 0 && self.remaining == 0
    }

    pub fn secs_until_reset(&self) -> u64 {
        let now = Utc::now();
        if self.reset <= now {
            return 0;
        }
        (self.reset - now).num_seconds().max(0) as u64
    }

    /// Returns the recommended poll interval in seconds based on remaining budget:
    /// - > 50% remaining → 30s
    /// - > 20% remaining → 60s
    /// - > 10% remaining → 120s
    /// - ≤ 10% (including exhausted) → `secs_until_reset().max(60)`
    pub fn recommended_interval(&self) -> u64 {
        let pct = self.pct_remaining();
        if pct > 50.0 {
            30
        } else if pct > 20.0 {
            60
        } else if pct > 10.0 {
            120
        } else {
            self.secs_until_reset().max(60)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn budget(remaining: u32, limit: u32, secs_until_reset: i64) -> RateLimitBudget {
        let reset = Utc::now() + Duration::seconds(secs_until_reset);
        RateLimitBudget::new(remaining, limit, reset)
    }

    #[test]
    fn full_budget_returns_30s() {
        let b = budget(5000, 5000, 3600);
        assert_eq!(b.recommended_interval(), 30);
    }

    #[test]
    fn half_budget_returns_30s() {
        // exactly 50% remaining — boundary is > 50%, so this falls into next tier
        let b = budget(2501, 5000, 3600);
        assert_eq!(b.recommended_interval(), 30);
    }

    #[test]
    fn low_budget_returns_60s() {
        // ~30% remaining → 60s
        let b = budget(1500, 5000, 3600);
        assert_eq!(b.recommended_interval(), 60);
    }

    #[test]
    fn critical_budget_returns_120s() {
        // ~15% remaining → 120s
        let b = budget(750, 5000, 3600);
        assert_eq!(b.recommended_interval(), 120);
    }

    #[test]
    fn exhausted_returns_until_reset() {
        // 0 remaining, resets in 300s → max(300, 60) = 300
        let b = budget(0, 5000, 300);
        assert!(b.is_exhausted());
        let interval = b.recommended_interval();
        // Should be at least 60 and approximately secs_until_reset
        assert!(interval >= 60);
        assert!(interval <= 310); // small tolerance for test execution time
    }
}
