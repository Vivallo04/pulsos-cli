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
use pulsos_core::domain::deployment::{DeploymentEvent, DeploymentStatus, Platform};
use pulsos_core::domain::health::HealthCalculator;
use pulsos_core::domain::project::{Confidence, CorrelatedEvent};
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

        // Build correlated events.
        let correlated = correlate_events(&all_events);

        // Compute health scores per correlation.
        let health_scores = compute_health_scores(&config.correlations, &all_events);

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

/// Group events by commit SHA to build correlated events.
///
/// Events with the same SHA are grouped together; events without a SHA
/// are treated as standalone (one CorrelatedEvent each).
fn correlate_events(events: &[DeploymentEvent]) -> Vec<CorrelatedEvent> {
    let mut by_sha: HashMap<String, Vec<&DeploymentEvent>> = HashMap::new();
    let mut no_sha: Vec<&DeploymentEvent> = Vec::new();

    for event in events {
        if let Some(ref sha) = event.commit_sha {
            by_sha.entry(sha.clone()).or_default().push(event);
        } else {
            no_sha.push(event);
        }
    }

    let mut correlated: Vec<CorrelatedEvent> = Vec::new();

    // Events grouped by SHA.
    for (sha, group) in &by_sha {
        let github = group
            .iter()
            .find(|e| e.platform == Platform::GitHub)
            .cloned()
            .cloned();
        let railway = group
            .iter()
            .find(|e| e.platform == Platform::Railway)
            .cloned()
            .cloned();
        let vercel = group
            .iter()
            .find(|e| e.platform == Platform::Vercel)
            .cloned()
            .cloned();

        let platform_count =
            github.is_some() as u8 + railway.is_some() as u8 + vercel.is_some() as u8;
        let confidence = if platform_count >= 2 {
            Confidence::Exact
        } else {
            Confidence::High
        };

        let timestamp = group
            .iter()
            .map(|e| e.created_at)
            .min()
            .unwrap_or_else(Utc::now);

        correlated.push(CorrelatedEvent {
            commit_sha: Some(sha.clone()),
            github,
            railway,
            vercel,
            confidence,
            timestamp,
        });
    }

    // Events without SHA — standalone.
    for event in &no_sha {
        let (github, railway, vercel) = match event.platform {
            Platform::GitHub => (Some((*event).clone()), None, None),
            Platform::Railway => (None, Some((*event).clone()), None),
            Platform::Vercel => (None, None, Some((*event).clone())),
        };

        correlated.push(CorrelatedEvent {
            commit_sha: None,
            github,
            railway,
            vercel,
            confidence: Confidence::Unmatched,
            timestamp: event.created_at,
        });
    }

    // Sort by timestamp descending.
    correlated.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    correlated
}

/// Compute per-project health scores using correlation config and fetched events.
fn compute_health_scores(
    correlations: &[CorrelationConfig],
    events: &[DeploymentEvent],
) -> Vec<(String, u8)> {
    let mut scores = Vec::new();

    for corr in correlations {
        // Gather GitHub runs for this correlation.
        let github_runs: Vec<DeploymentStatus> = if let Some(ref repo) = corr.github_repo {
            events
                .iter()
                .filter(|e| {
                    e.platform == Platform::GitHub
                        && e.metadata.workflow_name.is_some()
                        && event_matches_resource(e, repo)
                })
                .take(10)
                .map(|e| e.status.clone())
                .collect()
        } else {
            Vec::new()
        };

        // Latest Railway status.
        let railway_status = corr.railway_project.as_ref().and_then(|project_id| {
            events
                .iter()
                .filter(|e| {
                    e.platform == Platform::Railway && event_matches_resource(e, project_id)
                })
                .max_by_key(|e| e.created_at)
                .map(|e| e.status.clone())
        });

        // Latest Vercel status.
        let vercel_status = corr.vercel_project.as_ref().and_then(|project_id| {
            events
                .iter()
                .filter(|e| e.platform == Platform::Vercel && event_matches_resource(e, project_id))
                .max_by_key(|e| e.created_at)
                .map(|e| e.status.clone())
        });

        let score = HealthCalculator::compute(&github_runs, railway_status, vercel_status);
        scores.push((corr.name.clone(), score));
    }

    scores
}

/// Check if a deployment event matches a tracked resource identifier.
///
/// For GitHub, the event ID contains the repo info or the metadata does.
/// For Railway/Vercel, we match on the event id prefix or metadata fields.
fn event_matches_resource(event: &DeploymentEvent, resource_id: &str) -> bool {
    // The event ID is typically prefixed with or contains the resource identifier.
    // For a best-effort match, check if the resource_id appears in the event id
    // or in relevant metadata fields.
    if event.id.contains(resource_id) {
        return true;
    }

    match event.platform {
        Platform::GitHub => {
            // GitHub events from fetch_events include the repo in the ID pattern
            // e.g., "myorg/my-saas:run:12345"
            event.id.starts_with(resource_id)
        }
        Platform::Railway => {
            // Railway metadata may contain the service or project identifiers
            event
                .metadata
                .service_name
                .as_deref()
                .is_some_and(|s| resource_id.contains(s))
        }
        Platform::Vercel => {
            // Vercel metadata may reference the project
            event
                .metadata
                .deploy_target
                .as_deref()
                .is_some_and(|t| resource_id.contains(t))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsos_core::domain::deployment::EventMetadata;

    fn github_event(sha: &str, status: DeploymentStatus) -> DeploymentEvent {
        DeploymentEvent {
            id: format!("myorg/my-saas:run:{sha}"),
            platform: Platform::GitHub,
            status,
            commit_sha: Some(sha.into()),
            branch: Some("main".into()),
            title: Some("CI".into()),
            actor: Some("vivallo".into()),
            created_at: Utc::now(),
            updated_at: None,
            duration_secs: Some(42),
            url: None,
            metadata: EventMetadata {
                workflow_name: Some("CI".into()),
                trigger_event: Some("push".into()),
                ..Default::default()
            },
        }
    }

    fn railway_event(sha: Option<&str>, status: DeploymentStatus) -> DeploymentEvent {
        DeploymentEvent {
            id: "rw-deploy-1".into(),
            platform: Platform::Railway,
            status,
            commit_sha: sha.map(Into::into),
            branch: None,
            title: None,
            actor: None,
            created_at: Utc::now(),
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata {
                service_name: Some("api".into()),
                environment_name: Some("production".into()),
                ..Default::default()
            },
        }
    }

    fn vercel_event(sha: Option<&str>, status: DeploymentStatus) -> DeploymentEvent {
        DeploymentEvent {
            id: "vc-deploy-1".into(),
            platform: Platform::Vercel,
            status,
            commit_sha: sha.map(Into::into),
            branch: Some("main".into()),
            title: Some("my-saas-web".into()),
            actor: Some("vivallo".into()),
            created_at: Utc::now(),
            updated_at: None,
            duration_secs: Some(30),
            url: None,
            metadata: EventMetadata {
                deploy_target: Some("production".into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn correlate_events_groups_by_sha() {
        let events = vec![
            github_event("abc123", DeploymentStatus::Success),
            vercel_event(Some("abc123"), DeploymentStatus::Success),
            railway_event(None, DeploymentStatus::InProgress),
        ];

        let correlated = correlate_events(&events);

        // One group with SHA "abc123" (GitHub + Vercel), one standalone (Railway)
        assert_eq!(correlated.len(), 2);

        let sha_group = correlated
            .iter()
            .find(|c| c.commit_sha.as_deref() == Some("abc123"))
            .unwrap();
        assert!(sha_group.github.is_some());
        assert!(sha_group.vercel.is_some());
        assert!(sha_group.railway.is_none());
        assert_eq!(sha_group.confidence, Confidence::Exact);

        let standalone = correlated.iter().find(|c| c.commit_sha.is_none()).unwrap();
        assert!(standalone.railway.is_some());
        assert_eq!(standalone.confidence, Confidence::Unmatched);
    }

    #[test]
    fn correlate_events_single_sha_is_high_confidence() {
        let events = vec![github_event("abc123", DeploymentStatus::Success)];

        let correlated = correlate_events(&events);
        assert_eq!(correlated.len(), 1);
        assert_eq!(correlated[0].confidence, Confidence::High);
    }

    #[test]
    fn correlate_events_empty() {
        let correlated = correlate_events(&[]);
        assert!(correlated.is_empty());
    }

    #[test]
    fn health_scores_from_correlations() {
        let correlations = vec![CorrelationConfig {
            name: "my-saas".into(),
            github_repo: Some("myorg/my-saas".into()),
            railway_project: None,
            railway_workspace: None,
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: HashMap::new(),
        }];

        let events = vec![
            github_event("abc123", DeploymentStatus::Success),
            github_event("def456", DeploymentStatus::Success),
            github_event("ghi789", DeploymentStatus::Failed),
        ];

        let scores = compute_health_scores(&correlations, &events);
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].0, "my-saas");
        // 2 success, 1 failure in 3 runs → ~67% → 67
        assert_eq!(scores[0].1, 67);
    }

    #[test]
    fn health_scores_empty_correlations() {
        let scores = compute_health_scores(&[], &[]);
        assert!(scores.is_empty());
    }

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
