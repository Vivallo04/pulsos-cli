//! Real-time telemetry types — populated at runtime, never persisted.
//!
//! Two complementary monitoring strategies:
//! - `ResourceMetrics`: Whitebox container stats (CPU, RAM, network) from Railway's GraphQL API.
//! - `EndpointHealth`: Blackbox TTFB/uptime probes from the internal Ping Engine.
//!
//! Both are normalized into `ProjectTelemetry`, stored in `DataSnapshot.telemetry`.

use chrono::{DateTime, Utc};
use std::collections::VecDeque;

/// Container/server resource utilization — primarily populated by Railway.
#[derive(Debug, Clone)]
pub struct ResourceMetrics {
    pub cpu_percent: Option<f64>,
    pub memory_used_mb: Option<f64>,
    pub memory_limit_mb: Option<f64>,
    pub network_rx_bytes: Option<u64>,
    pub network_tx_bytes: Option<u64>,
    pub timestamp: DateTime<Utc>,
}

impl Default for ResourceMetrics {
    fn default() -> Self {
        Self {
            cpu_percent: None,
            memory_used_mb: None,
            memory_limit_mb: None,
            network_rx_bytes: None,
            network_tx_bytes: None,
            timestamp: Utc::now(),
        }
    }
}

/// Blackbox endpoint health — result of a single TTFB/uptime probe.
#[derive(Debug, Clone)]
pub struct EndpointHealth {
    pub url: String,
    pub is_up: bool,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u64>,
    pub checked_at: DateTime<Utc>,
}

/// Rolling telemetry for a single project. Stored in `DataSnapshot.telemetry` keyed by
/// correlation name. Not persisted — rebuilt fresh each TUI session.
#[derive(Debug, Clone, Default)]
pub struct ProjectTelemetry {
    /// Latest Railway container stats (empty until first metrics poll succeeds).
    pub current_resources: ResourceMetrics,
    /// Ping history ring buffer — up to `HISTORY_CAP` entries, newest at the back.
    pub endpoint_history: VecDeque<EndpointHealth>,
}

impl ProjectTelemetry {
    pub const HISTORY_CAP: usize = 100;

    pub fn push_ping(&mut self, ping: EndpointHealth) {
        self.endpoint_history.push_back(ping);
        if self.endpoint_history.len() > Self::HISTORY_CAP {
            self.endpoint_history.pop_front();
        }
    }

    /// Latest ping result, if any.
    pub fn latest_ping(&self) -> Option<&EndpointHealth> {
        self.endpoint_history.back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_ping_caps_at_history_cap() {
        let mut tel = ProjectTelemetry::default();
        for i in 0..=ProjectTelemetry::HISTORY_CAP + 5 {
            tel.push_ping(EndpointHealth {
                url: format!("https://example.com/{i}"),
                is_up: true,
                status_code: Some(200),
                latency_ms: Some(i as u64),
                checked_at: Utc::now(),
            });
        }
        assert_eq!(tel.endpoint_history.len(), ProjectTelemetry::HISTORY_CAP);
    }

    #[test]
    fn latest_ping_returns_newest() {
        let mut tel = ProjectTelemetry::default();
        assert!(tel.latest_ping().is_none());
        tel.push_ping(EndpointHealth {
            url: "https://a.com".into(),
            is_up: true,
            status_code: Some(200),
            latency_ms: Some(50),
            checked_at: Utc::now(),
        });
        tel.push_ping(EndpointHealth {
            url: "https://b.com".into(),
            is_up: false,
            status_code: None,
            latency_ms: None,
            checked_at: Utc::now(),
        });
        assert_eq!(tel.latest_ping().unwrap().url, "https://b.com");
    }
}
