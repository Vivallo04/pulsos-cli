//! Blackbox endpoint health prober — measures TTFB/latency and uptime.
//!
//! Used as the primary telemetry source for Vercel (which does not expose
//! container-level metrics) and as a supplementary signal for Railway.
//!
//! Probes hit the user's own deployed URLs — they do not consume platform API
//! rate limits, so a shorter poll interval (e.g. 8s) is safe.

use std::time::{Duration, Instant};

use crate::error::PulsosError;
use crate::domain::metrics::EndpointHealth;

/// Lightweight HTTP prober. Reuses a single `reqwest::Client` across pings.
pub struct PingEngine {
    client: reqwest::Client,
}

impl PingEngine {
    /// Create a new `PingEngine` with a 5-second per-request timeout.
    pub fn new() -> Result<Self, PulsosError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .user_agent("pulsos-ping/0.1.0")
            .build()
            .map_err(|e| {
                PulsosError::Other(anyhow::anyhow!(
                    "Failed to build ping HTTP client: {e}"
                ))
            })?;

        Ok(Self { client })
    }

    /// Probe a URL and return an `EndpointHealth` with latency and status.
    ///
    /// Prepends `https://` if no scheme is present. Never panics — all errors
    /// are mapped to `is_up: false` with no status code or latency.
    pub async fn ping(&self, url: &str) -> EndpointHealth {
        let target = if url.starts_with("http://") || url.starts_with("https://") {
            url.to_owned()
        } else {
            format!("https://{url}")
        };

        let start = Instant::now();

        match self.client.get(&target).send().await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let status = resp.status();
                EndpointHealth {
                    url: target,
                    is_up: status.is_success() || status.is_redirection(),
                    status_code: Some(status.as_u16()),
                    latency_ms: Some(latency_ms),
                    checked_at: chrono::Utc::now(),
                }
            }
            Err(_) => EndpointHealth {
                url: target,
                is_up: false,
                status_code: None,
                latency_ms: None,
                checked_at: chrono::Utc::now(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_engine_creates_without_panic() -> Result<(), PulsosError> {
        let _engine = PingEngine::new()?;
        Ok(())
    }

    // Note: network tests are not run in unit test suite.
    // Integration tests cover live pings in a controlled environment.
}
