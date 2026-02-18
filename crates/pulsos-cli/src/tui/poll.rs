//! Background poller — periodically fetches platform data and sends snapshots.
//!
//! Architecture:
//! - Runs as a tokio task, sends `DataSnapshot` through a `watch::Sender`
//! - Per-platform throttle: GitHub 30s, Railway/Vercel 15s
//! - Supports force-refresh (bypass throttle on user request)
//! - Builds correlated events by grouping on commit_sha
//! - Computes health scores via `HealthCalculator`
//! - Maintains a ring buffer of health history for sparklines

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::watch;

use pulsos_core::auth::credential_store::KeyringStore;
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::types::{CorrelationConfig, PulsosConfig};
use pulsos_core::correlation;
use pulsos_core::domain::deployment::DeploymentEvent;
use pulsos_core::domain::health;
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::{PlatformAdapter, TrackedResource};

use super::app::DataSnapshot;

/// Maximum number of health history entries per project (for sparklines).
const HEALTH_HISTORY_CAP: usize = 20;

/// Per-platform throttle intervals.
const GITHUB_THROTTLE: Duration = Duration::from_secs(30);
const RAILWAY_THROTTLE: Duration = Duration::from_secs(15);
const VERCEL_THROTTLE: Duration = Duration::from_secs(15);

/// Tracked resources grouped by platform, built from config correlations.
pub struct PlatformResources {
    pub github: Vec<TrackedResource>,
    pub railway: Vec<TrackedResource>,
    pub vercel: Vec<TrackedResource>,
}

impl PlatformResources {
    /// Build tracked resources from correlation config entries.
    pub fn from_correlations(correlations: &[CorrelationConfig]) -> Self {
        let mut github = Vec::new();
        let mut railway = Vec::new();
        let mut vercel = Vec::new();

        for corr in correlations {
            if let Some(ref repo) = corr.github_repo {
                github.push(TrackedResource {
                    platform_id: repo.clone(),
                    display_name: repo.split('/').next_back().unwrap_or(repo).to_string(),
                    group: None,
                });
            }
            if let Some(ref project) = corr.railway_project {
                railway.push(TrackedResource {
                    platform_id: project.clone(),
                    display_name: corr.name.clone(),
                    group: corr.railway_workspace.clone(),
                });
            }
            if let Some(ref project) = corr.vercel_project {
                vercel.push(TrackedResource {
                    platform_id: project.clone(),
                    display_name: corr.name.clone(),
                    group: corr.vercel_team.clone(),
                });
            }
        }

        Self {
            github,
            railway,
            vercel,
        }
    }
}

/// Run the background poller loop.
///
/// Sends `DataSnapshot` updates through `tx` whenever new data is available.
/// Checks `force_rx` for force-refresh signals from the UI.
/// Stops when `tx` is closed (all receivers dropped).
pub async fn run_poller(
    config: PulsosConfig,
    tx: watch::Sender<DataSnapshot>,
    mut force_rx: tokio::sync::mpsc::Receiver<()>,
) {
    let resources = PlatformResources::from_correlations(&config.correlations);

    let cache = match CacheStore::open_default() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            tracing::error!("Failed to open cache store: {e}");
            return;
        }
    };

    let store = Arc::new(KeyringStore::new());
    let resolver = TokenResolver::new(store, config.auth.token_detection.clone());

    let poll_interval = Duration::from_secs(config.tui.refresh_interval.max(1));

    let mut last_github = Instant::now() - GITHUB_THROTTLE;
    let mut last_railway = Instant::now() - RAILWAY_THROTTLE;
    let mut last_vercel = Instant::now() - VERCEL_THROTTLE;

    // Health history ring buffer: project_name → Vec<u8>
    let mut health_history: HashMap<String, Vec<u8>> = HashMap::new();

    loop {
        let force_refresh = tokio::select! {
            _ = tokio::time::sleep(poll_interval) => false,
            msg = force_rx.recv() => {
                match msg {
                    Some(()) => true,
                    None => return, // channel closed, UI exited
                }
            }
        };

        // Check if tx is closed (all receivers dropped).
        if tx.is_closed() {
            return;
        }

        let now = Instant::now();
        let mut all_events: Vec<DeploymentEvent> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        // GitHub
        if !resources.github.is_empty()
            && (force_refresh || now.duration_since(last_github) >= GITHUB_THROTTLE)
        {
            if let Some(token) = resolver.resolve(&PlatformKind::GitHub) {
                let client = GitHubClient::new(token, cache.clone());
                match client.fetch_events(&resources.github).await {
                    Ok(events) => {
                        all_events.extend(events);
                        last_github = now;
                    }
                    Err(e) => warnings.push(format!("GitHub: {}", e.user_message())),
                }
            } else {
                warnings.push("GitHub: no token available".into());
            }
        }

        // Railway
        if !resources.railway.is_empty()
            && (force_refresh || now.duration_since(last_railway) >= RAILWAY_THROTTLE)
        {
            if let Some(token) = resolver.resolve(&PlatformKind::Railway) {
                let client = RailwayClient::new(token, cache.clone());
                match client.fetch_events(&resources.railway).await {
                    Ok(events) => {
                        all_events.extend(events);
                        last_railway = now;
                    }
                    Err(e) => warnings.push(format!("Railway: {}", e.user_message())),
                }
            } else {
                warnings.push("Railway: no token available".into());
            }
        }

        // Vercel
        if !resources.vercel.is_empty()
            && (force_refresh || now.duration_since(last_vercel) >= VERCEL_THROTTLE)
        {
            if let Some(token) = resolver.resolve(&PlatformKind::Vercel) {
                let client = VercelClient::new(token, cache.clone());
                match client.fetch_events(&resources.vercel).await {
                    Ok(events) => {
                        all_events.extend(events);
                        last_vercel = now;
                    }
                    Err(e) => warnings.push(format!("Vercel: {}", e.user_message())),
                }
            } else {
                warnings.push("Vercel: no token available".into());
            }
        }

        // Sort events by created_at descending.
        all_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Build correlated events using core correlation engine.
        let correlated = correlation::correlate_all(&config.correlations, &all_events);

        // Compute health scores per correlation.
        let health_scores =
            health::compute_project_health_scores(&config.correlations, &all_events);

        // Update health history ring buffer.
        for (name, score) in &health_scores {
            let history = health_history.entry(name.clone()).or_default();
            history.push(*score);
            if history.len() > HEALTH_HISTORY_CAP {
                history.remove(0);
            }
        }

        let health_history_snapshot: Vec<(String, Vec<u8>)> = health_history
            .iter()
            .map(|(name, history)| (name.clone(), history.clone()))
            .collect();

        let snapshot = DataSnapshot {
            events: all_events,
            correlated,
            health_scores,
            health_history: health_history_snapshot,
            warnings,
            fetched_at: Utc::now(),
        };

        // Send snapshot. If send fails, all receivers are gone — exit.
        if tx.send(snapshot).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_resources_from_correlations() {
        let correlations = vec![
            CorrelationConfig {
                name: "my-saas".into(),
                github_repo: Some("myorg/my-saas".into()),
                railway_project: Some("proj:svc:env".into()),
                railway_workspace: Some("lambda-prod".into()),
                railway_environment: None,
                vercel_project: Some("prj-001".into()),
                vercel_team: Some("Lambda".into()),
                branch_mapping: HashMap::new(),
            },
            CorrelationConfig {
                name: "api-core".into(),
                github_repo: Some("myorg/api-core".into()),
                railway_project: None,
                railway_workspace: None,
                railway_environment: None,
                vercel_project: None,
                vercel_team: None,
                branch_mapping: HashMap::new(),
            },
        ];

        let resources = PlatformResources::from_correlations(&correlations);
        assert_eq!(resources.github.len(), 2);
        assert_eq!(resources.railway.len(), 1);
        assert_eq!(resources.vercel.len(), 1);
        assert_eq!(resources.github[0].platform_id, "myorg/my-saas");
        assert_eq!(resources.github[0].display_name, "my-saas");
        assert_eq!(resources.railway[0].display_name, "my-saas");
        assert_eq!(resources.vercel[0].group, Some("Lambda".into()));
    }

    #[test]
    fn health_history_ring_buffer_caps_at_limit() {
        let mut history: HashMap<String, Vec<u8>> = HashMap::new();

        // Simulate adding more than HEALTH_HISTORY_CAP entries.
        for i in 0..25 {
            let entry = history.entry("test-project".into()).or_default();
            entry.push(i as u8);
            if entry.len() > HEALTH_HISTORY_CAP {
                entry.remove(0);
            }
        }

        let entry = &history["test-project"];
        assert_eq!(entry.len(), HEALTH_HISTORY_CAP);
        // Should contain the last 20 entries: 5..25
        assert_eq!(entry[0], 5);
        assert_eq!(entry[19], 24);
    }
}
