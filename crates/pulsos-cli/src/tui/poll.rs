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

use pulsos_core::analytics::dora::DoraCalculator;
use pulsos_core::auth::credential_store::{CredentialStore, FallbackStore};
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::types::{CorrelationConfig, PulsosConfig};
use pulsos_core::correlation;
use pulsos_core::domain::analytics::DoraMetrics;
use pulsos_core::domain::deployment::DeploymentEvent;
use pulsos_core::domain::health;
use pulsos_core::domain::metrics::ProjectTelemetry;
use pulsos_core::domain::project::CorrelatedEvent;
use pulsos_core::health::pinger::PingEngine;
use pulsos_core::health::{check_all_platforms_health, PlatformHealthReport};
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::{PlatformAdapter, TrackedResource};
use pulsos_core::scheduler::budget::RateLimitBudget;
use pulsos_core::scheduler::poller::{stagger, BATCH_DELAY_SECS, BATCH_SIZE};

use super::app::DataSnapshot;

#[derive(Debug, Clone)]
pub enum PollerCommand {
    ForceRefresh,
    ReplaceConfig(PulsosConfig),
}

/// Maximum number of health history entries per project (for sparklines).
const HEALTH_HISTORY_CAP: usize = 20;
/// Maximum number of correlated events kept in the session-level DORA history buffer.
const DORA_HISTORY_CAP: usize = 200;

/// Per-platform throttle intervals.
const GITHUB_THROTTLE: Duration = Duration::from_secs(30);
const RAILWAY_THROTTLE: Duration = Duration::from_secs(15);
const VERCEL_THROTTLE: Duration = Duration::from_secs(15);
const HEALTH_THROTTLE: Duration = Duration::from_secs(30);
/// Telemetry throttle intervals.
const METRICS_THROTTLE: Duration = Duration::from_secs(30);
const PING_THROTTLE: Duration = Duration::from_secs(8);

fn combine_events(
    github: &[DeploymentEvent],
    railway: &[DeploymentEvent],
    vercel: &[DeploymentEvent],
) -> Vec<DeploymentEvent> {
    let mut events = Vec::with_capacity(github.len() + railway.len() + vercel.len());
    events.extend_from_slice(github);
    events.extend_from_slice(railway);
    events.extend_from_slice(vercel);
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    events
}

async fn wait_for_next_cycle(
    first_cycle: &mut bool,
    poll_interval: Duration,
    command_rx: &mut tokio::sync::mpsc::Receiver<PollerCommand>,
    config: &mut PulsosConfig,
    resources: &mut PlatformResources,
) -> Option<bool> {
    if *first_cycle {
        *first_cycle = false;
        return Some(false);
    }

    loop {
        tokio::select! {
            _ = tokio::time::sleep(poll_interval) => return Some(false),
            msg = command_rx.recv() => {
                match msg {
                    Some(PollerCommand::ForceRefresh) => return Some(true),
                    Some(PollerCommand::ReplaceConfig(new_config)) => {
                        *config = new_config;
                        *resources = PlatformResources::from_correlations(&config.correlations);
                        return Some(true);
                    }
                    None => return None,
                }
            }
        }
    }
}

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
    mut config: PulsosConfig,
    tx: watch::Sender<DataSnapshot>,
    mut command_rx: tokio::sync::mpsc::Receiver<PollerCommand>,
) {
    let mut resources = PlatformResources::from_correlations(&config.correlations);

    let cache = Arc::new(CacheStore::open_or_temporary());

    let store: Arc<dyn CredentialStore> = match FallbackStore::new() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!("Credential store error: {e}");
            return;
        }
    };
    let store = store;

    let poll_interval = Duration::from_secs(config.tui.refresh_interval.max(1));
    let mut first_cycle = true;

    // github_throttle starts at the static constant and is updated adaptively each cycle.
    let mut github_throttle = GITHUB_THROTTLE;
    let mut last_github = Instant::now() - github_throttle;
    let mut last_railway = Instant::now() - RAILWAY_THROTTLE;
    let mut last_vercel = Instant::now() - VERCEL_THROTTLE;
    let mut last_github_events: Vec<DeploymentEvent> = Vec::new();
    let mut last_railway_events: Vec<DeploymentEvent> = Vec::new();
    let mut last_vercel_events: Vec<DeploymentEvent> = Vec::new();
    let mut last_cycle_warnings: Vec<String> = Vec::new();
    let mut last_platform_health: Vec<PlatformHealthReport> = Vec::new();
    let mut last_cycle_completed_at = Utc::now();
    let mut last_health_check_at = Instant::now() - HEALTH_THROTTLE;

    // Health history ring buffer: project_name → Vec<u8>
    let mut health_history: HashMap<String, Vec<u8>> = HashMap::new();

    // DORA history ring buffer: accumulates correlated events across poll cycles (cap 200).
    let mut dora_history: Vec<CorrelatedEvent> = Vec::new();

    // Telemetry state: project_name → ProjectTelemetry (Railway metrics + pings)
    let mut last_telemetry: HashMap<String, ProjectTelemetry> = HashMap::new();
    let mut last_metrics_at = Instant::now() - METRICS_THROTTLE;
    let mut last_ping_at = Instant::now() - PING_THROTTLE;
    let ping_engine = PingEngine::new();

    loop {
        let mut force_refresh = match wait_for_next_cycle(
            &mut first_cycle,
            poll_interval,
            &mut command_rx,
            &mut config,
            &mut resources,
        )
        .await
        {
                Some(v) => v,
                None => return, // channel closed, UI exited
            };

        while let Ok(command) = command_rx.try_recv() {
            match command {
                PollerCommand::ForceRefresh => force_refresh = true,
                PollerCommand::ReplaceConfig(new_config) => {
                    config = new_config;
                    resources = PlatformResources::from_correlations(&config.correlations);
                    force_refresh = true;
                }
            }
        }

        // Check if tx is closed (all receivers dropped).
        if tx.is_closed() {
            return;
        }

        let cycle_started_at = Utc::now();
        let pre_events = combine_events(
            &last_github_events,
            &last_railway_events,
            &last_vercel_events,
        );
        let pre_correlated = correlation::correlate_all(&config.correlations, &pre_events);
        let pre_health_scores =
            health::compute_project_health_scores(&config.correlations, &pre_events);
        let pre_health_breakdowns =
            health::compute_project_health_breakdowns(&config.correlations, &pre_events);
        let pre_health_history: Vec<(String, Vec<u8>)> = health_history
            .iter()
            .map(|(name, history)| (name.clone(), history.clone()))
            .collect();

        let syncing_snapshot = DataSnapshot {
            events: pre_events,
            correlated: pre_correlated,
            health_scores: pre_health_scores,
            health_breakdowns: pre_health_breakdowns,
            health_history: pre_health_history,
            warnings: last_cycle_warnings.clone(),
            platform_health: last_platform_health.clone(),
            telemetry: last_telemetry.clone(),
            dora_metrics: DoraMetrics::default(),
            dora_history_count: 0,
            is_syncing: true,
            fetched_at: Utc::now(),
            last_cycle_started_at: cycle_started_at,
            last_cycle_completed_at,
        };
        if tx.send(syncing_snapshot).is_err() {
            return;
        }

        let now = Instant::now();
        let mut warnings: Vec<String> = Vec::new();
        let resolver = TokenResolver::new(store.clone(), config.auth.token_detection.clone());

        // GitHub
        if !resources.github.is_empty()
            && (force_refresh || now.duration_since(last_github) >= github_throttle)
        {
            if let Some(token) = resolver.resolve(&PlatformKind::GitHub) {
                let client = GitHubClient::new(token, cache.clone());
                let mut github_events: Vec<DeploymentEvent> = Vec::new();
                let mut github_failed = false;

                // Staggered fetch: process repos in batches to avoid burst rate-limit hits.
                let results = stagger(
                    &resources.github,
                    BATCH_SIZE,
                    BATCH_DELAY_SECS,
                    |resource| {
                        let client = &client;
                        async move { client.fetch_events(std::slice::from_ref(resource)).await }
                    },
                )
                .await;

                for result in results {
                    match result {
                        Ok(evts) => github_events.extend(evts),
                        Err(e) => {
                            github_failed = true;
                            warnings.push(format!("GitHub: {}", e.user_message()));
                        }
                    }
                }
                if !github_failed {
                    last_github_events = github_events;
                    last_github = now;
                }

                // Update adaptive poll interval based on current rate-limit budget.
                if let Ok(rl_info) = client.rate_limit_status().await {
                    let budget = RateLimitBudget::from_rate_limit_info(&rl_info);
                    github_throttle = Duration::from_secs(budget.recommended_interval());
                    if budget.is_exhausted() {
                        warnings.push(format!(
                            "GitHub rate limit exhausted. Showing cached data. Resets in {}s.",
                            budget.secs_until_reset()
                        ));
                    }
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
                        last_railway_events = events;
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
                        last_vercel_events = events;
                        last_vercel = now;
                    }
                    Err(e) => warnings.push(format!("Vercel: {}", e.user_message())),
                }
            } else {
                warnings.push("Vercel: no token available".into());
            }
        }

        if force_refresh || now.duration_since(last_health_check_at) >= HEALTH_THROTTLE {
            last_platform_health = check_all_platforms_health(&config, &resolver, &cache).await;
            last_health_check_at = now;
        }

        // Railway container metrics (whitebox — uses Railway GraphQL metrics API)
        if !resources.railway.is_empty()
            && (force_refresh || now.duration_since(last_metrics_at) >= METRICS_THROTTLE)
        {
            if let Some(token) = resolver.resolve(&PlatformKind::Railway) {
                let client = RailwayClient::new(token, cache.clone());
                for resource in &resources.railway {
                    let metrics = client.fetch_service_metrics(&resource.platform_id).await;
                    last_telemetry
                        .entry(resource.display_name.clone())
                        .or_default()
                        .current_resources = metrics;
                }
                last_metrics_at = now;
            }
        }

        // Ping engine (blackbox — probes deployed URLs for TTFB/uptime)
        // Hits user's own servers, not platform APIs, so uses a tighter interval.
        if force_refresh || now.duration_since(last_ping_at) >= PING_THROTTLE {
            for corr in &config.correlations {
                // Prefer the most recent Vercel URL, fall back to Railway static_url.
                let url = last_vercel_events
                    .iter()
                    .chain(last_railway_events.iter())
                    .filter(|e| {
                        e.metadata
                            .source_id
                            .as_deref()
                            .map(|id| {
                                corr.vercel_project.as_deref() == Some(id)
                                    || corr.railway_project.as_deref() == Some(id)
                                    || id.contains(
                                        corr.vercel_project.as_deref().unwrap_or(""),
                                    )
                            })
                            .unwrap_or(false)
                    })
                    .find_map(|e| e.url.clone().or_else(|| e.metadata.preview_url.clone()))
                    .or_else(|| {
                        // Fallback: any recent event that belongs to this correlation
                        last_vercel_events
                            .iter()
                            .chain(last_railway_events.iter())
                            .find_map(|e| e.url.clone().or_else(|| e.metadata.preview_url.clone()))
                    });

                if let Some(u) = url {
                    let ping = ping_engine.ping(&u).await;
                    last_telemetry
                        .entry(corr.name.clone())
                        .or_default()
                        .push_ping(ping);
                }
            }
            last_ping_at = now;
        }

        let all_events = combine_events(
            &last_github_events,
            &last_railway_events,
            &last_vercel_events,
        );

        // Build correlated events using core correlation engine.
        let correlated = correlation::correlate_all(&config.correlations, &all_events);

        // Merge new correlated events into the DORA history buffer.
        // Deduplicate by (project_name, commit_sha). Evict oldest when cap exceeded.
        for event in &correlated {
            let already_present = dora_history.iter().any(|h| {
                h.project_name == event.project_name && h.commit_sha == event.commit_sha
            });
            if !already_present {
                dora_history.push(event.clone());
            }
        }
        if dora_history.len() > DORA_HISTORY_CAP {
            let excess = dora_history.len() - DORA_HISTORY_CAP;
            dora_history.drain(0..excess);
        }

        // Compute aggregate DORA metrics from accumulated history.
        let dora_metrics = DoraCalculator::compute(&dora_history);
        let dora_history_count = dora_history.len();

        // Compute health scores and breakdowns per correlation.
        let health_scores =
            health::compute_project_health_scores(&config.correlations, &all_events);
        let health_breakdowns =
            health::compute_project_health_breakdowns(&config.correlations, &all_events);

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

        let cycle_completed_at = Utc::now();
        let snapshot = DataSnapshot {
            events: all_events,
            correlated,
            health_scores,
            health_breakdowns,
            health_history: health_history_snapshot,
            warnings: warnings.clone(),
            platform_health: last_platform_health.clone(),
            telemetry: last_telemetry.clone(),
            dora_metrics,
            dora_history_count,
            is_syncing: false,
            fetched_at: cycle_completed_at,
            last_cycle_started_at: cycle_started_at,
            last_cycle_completed_at: cycle_completed_at,
        };
        last_cycle_warnings = warnings;
        last_cycle_completed_at = cycle_completed_at;

        // Send snapshot. If send fails, all receivers are gone — exit.
        if tx.send(snapshot).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, Utc};
    use pulsos_core::domain::deployment::{DeploymentStatus, EventMetadata, Platform};

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

    fn test_event(id: &str, platform: Platform, seconds_ago: i64) -> DeploymentEvent {
        let ts = Utc::now() - ChronoDuration::seconds(seconds_ago);
        DeploymentEvent {
            id: id.to_string(),
            platform,
            status: DeploymentStatus::Success,
            commit_sha: None,
            branch: None,
            title: Some(id.to_string()),
            actor: None,
            created_at: ts,
            updated_at: Some(ts),
            duration_secs: None,
            url: None,
            metadata: EventMetadata::default(),
            is_from_cache: false,
        }
    }

    #[test]
    fn combine_events_keeps_data_from_all_platform_caches() {
        let gh = vec![test_event("gh1", Platform::GitHub, 10)];
        let rw = vec![test_event("rw1", Platform::Railway, 5)];
        let vc = vec![test_event("vc1", Platform::Vercel, 1)];

        let combined = combine_events(&gh, &rw, &vc);
        assert_eq!(combined.len(), 3);
        assert_eq!(combined[0].id, "vc1");
        assert_eq!(combined[1].id, "rw1");
        assert_eq!(combined[2].id, "gh1");
    }

    #[test]
    fn combine_events_preserves_previous_platform_data_when_others_update() {
        let gh_cached = vec![test_event("gh_cached", Platform::GitHub, 20)];
        let rw_new = vec![test_event("rw_new", Platform::Railway, 1)];

        let combined = combine_events(&gh_cached, &rw_new, &[]);
        assert_eq!(combined.len(), 2);
        assert!(combined.iter().any(|e| e.id == "gh_cached"));
        assert!(combined.iter().any(|e| e.id == "rw_new"));
    }

    #[tokio::test]
    async fn first_cycle_does_not_wait_for_poll_interval() {
        let mut first_cycle = true;
        let poll_interval = Duration::from_secs(60);
        let (_tx, mut rx) = tokio::sync::mpsc::channel::<PollerCommand>(1);
        let mut config = PulsosConfig::default();
        let mut resources = PlatformResources::from_correlations(&config.correlations);

        let result = tokio::time::timeout(
            Duration::from_millis(1),
            wait_for_next_cycle(
                &mut first_cycle,
                poll_interval,
                &mut rx,
                &mut config,
                &mut resources,
            ),
        )
        .await
        .expect("first cycle should return immediately");

        assert_eq!(result, Some(false));
    }
}
