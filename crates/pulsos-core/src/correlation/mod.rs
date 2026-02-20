//! Correlation engine — links deployment events across platforms.
//!
//! The engine operates in two tiers:
//! 1. **SHA matching**: GitHub <-> Vercel events with the same commit SHA (`Exact`)
//! 2. **Timestamp heuristic**: Remaining events matched by proximity (`High` with
//!    explicit config mapping, `Low` without)
//!
//! Events that don't match anything become standalone `Unmatched` entries.

pub mod confidence;
pub mod heuristic;
pub mod sha_match;

use crate::config::types::CorrelationConfig;
use crate::domain::deployment::{DeploymentEvent, Platform};
use crate::domain::project::{Confidence, CorrelatedEvent};

use confidence::score_confidence;
use heuristic::{find_closest_by_timestamp, TIMESTAMP_WINDOW_SECS};
use sha_match::find_sha_matches;

/// Check if a deployment event belongs to a given correlation config project.
pub fn event_matches_project(event: &DeploymentEvent, config: &CorrelationConfig) -> bool {
    match event.platform {
        Platform::GitHub => {
            if let Some(ref repo) = config.github_repo {
                event.id.starts_with(repo)
            } else {
                false
            }
        }
        Platform::Railway => {
            if let Some(ref project_id) = config.railway_project {
                // Match on project ID prefix in the event id
                if event.id.contains(project_id) {
                    return true;
                }
                // Match on service name from metadata against project name
                if let Some(ref service) = event.metadata.service_name {
                    if project_id.contains(service) {
                        return true;
                    }
                }
                false
            } else {
                false
            }
        }
        Platform::Vercel => {
            if let Some(ref project_id) = config.vercel_project {
                // Match on project ID in the event id
                if event.id.contains(project_id) {
                    return true;
                }
                // Match on project name in the title
                if let Some(ref title) = event.title {
                    if title.contains(project_id) {
                        return true;
                    }
                }
                false
            } else {
                false
            }
        }
    }
}

/// Correlate events within a single project scope.
///
/// Algorithm:
/// 1. Partition events by platform
/// 2. SHA-match GitHub <-> Vercel -> `Exact`
/// 3. For each matched/unmatched GitHub event, heuristic-match against Railway
/// 4. For unmatched Vercel events, heuristic-match against Railway
/// 5. Remaining unmatched -> standalone
pub fn correlate_project_events(
    config: &CorrelationConfig,
    events: &[DeploymentEvent],
) -> Vec<CorrelatedEvent> {
    let github: Vec<&DeploymentEvent> = events
        .iter()
        .filter(|e| e.platform == Platform::GitHub)
        .collect();
    let railway: Vec<&DeploymentEvent> = events
        .iter()
        .filter(|e| e.platform == Platform::Railway)
        .collect();
    let vercel: Vec<&DeploymentEvent> = events
        .iter()
        .filter(|e| e.platform == Platform::Vercel)
        .collect();

    let mut result: Vec<CorrelatedEvent> = Vec::new();
    let mut claimed_github = vec![false; github.len()];
    let mut claimed_railway = vec![false; railway.len()];
    let mut claimed_vercel = vec![false; vercel.len()];

    let has_railway_mapping = config.railway_project.is_some();

    // Step 1: SHA-match GitHub <-> Vercel
    let sha_matches = find_sha_matches(&github, &vercel);
    for (gi, vi) in &sha_matches {
        claimed_github[*gi] = true;
        claimed_vercel[*vi] = true;

        // Try heuristic-match against Railway for this SHA group
        let railway_event = find_closest_by_timestamp(
            github[*gi],
            &railway,
            &claimed_railway,
            TIMESTAMP_WINDOW_SECS,
        )
        .map(|m| {
            claimed_railway[m.candidate_index] = true;
            railway[m.candidate_index].clone()
        });

        let timestamp = github[*gi].created_at.min(vercel[*vi].created_at).min(
            railway_event
                .as_ref()
                .map_or(github[*gi].created_at, |r| r.created_at),
        );

        let railway_conf = if railway_event.is_some() {
            score_confidence(false, true, has_railway_mapping)
        } else {
            Confidence::Unmatched
        };
        // Overall confidence: SHA match is Exact, but if railway was added by heuristic
        // the overall is still Exact (SHA is the primary signal)
        let _ = railway_conf;

        let is_stale = github[*gi].is_from_cache
            || railway_event.as_ref().map_or(false, |e| e.is_from_cache)
            || vercel[*vi].is_from_cache;

        result.push(CorrelatedEvent {
            commit_sha: github[*gi].commit_sha.clone(),
            github: Some(github[*gi].clone()),
            railway: railway_event,
            vercel: Some(vercel[*vi].clone()),
            confidence: Confidence::Exact,
            timestamp,
            is_stale,
        });
    }

    // Step 2: Unmatched GitHub events — try heuristic against Railway
    for (gi, gh_event) in github.iter().enumerate() {
        if claimed_github[gi] {
            continue;
        }
        claimed_github[gi] = true;

        let railway_event =
            find_closest_by_timestamp(gh_event, &railway, &claimed_railway, TIMESTAMP_WINDOW_SECS)
                .map(|m| {
                    claimed_railway[m.candidate_index] = true;
                    railway[m.candidate_index].clone()
                });

        let confidence = if railway_event.is_some() {
            score_confidence(false, true, has_railway_mapping)
        } else {
            Confidence::Unmatched
        };

        let timestamp = gh_event.created_at.min(
            railway_event
                .as_ref()
                .map_or(gh_event.created_at, |r| r.created_at),
        );

        let is_stale =
            gh_event.is_from_cache || railway_event.as_ref().map_or(false, |e| e.is_from_cache);

        result.push(CorrelatedEvent {
            commit_sha: gh_event.commit_sha.clone(),
            github: Some((*gh_event).clone()),
            railway: railway_event,
            vercel: None,
            confidence,
            timestamp,
            is_stale,
        });
    }

    // Step 3: Unmatched Vercel events — try heuristic against Railway
    for (vi, vc_event) in vercel.iter().enumerate() {
        if claimed_vercel[vi] {
            continue;
        }
        claimed_vercel[vi] = true;

        let railway_event =
            find_closest_by_timestamp(vc_event, &railway, &claimed_railway, TIMESTAMP_WINDOW_SECS)
                .map(|m| {
                    claimed_railway[m.candidate_index] = true;
                    railway[m.candidate_index].clone()
                });

        let confidence = if railway_event.is_some() {
            score_confidence(false, true, has_railway_mapping)
        } else {
            Confidence::Unmatched
        };

        let timestamp = vc_event.created_at.min(
            railway_event
                .as_ref()
                .map_or(vc_event.created_at, |r| r.created_at),
        );

        let is_stale =
            vc_event.is_from_cache || railway_event.as_ref().map_or(false, |e| e.is_from_cache);

        result.push(CorrelatedEvent {
            commit_sha: vc_event.commit_sha.clone(),
            github: None,
            railway: railway_event,
            vercel: Some((*vc_event).clone()),
            confidence,
            timestamp,
            is_stale,
        });
    }

    // Step 4: Remaining Railway events — standalone Unmatched
    for (ri, rw_event) in railway.iter().enumerate() {
        if claimed_railway[ri] {
            continue;
        }

        result.push(CorrelatedEvent {
            commit_sha: rw_event.commit_sha.clone(),
            github: None,
            railway: Some((*rw_event).clone()),
            vercel: None,
            confidence: Confidence::Unmatched,
            timestamp: rw_event.created_at,
            is_stale: rw_event.is_from_cache,
        });
    }

    result
}

/// Correlate all events across multiple project configurations.
///
/// For each config, filters events that belong to that project, then runs
/// `correlate_project_events`. Unmatched events (no config match) become
/// standalone `Unmatched` entries. Results are sorted by timestamp descending.
pub fn correlate_all(
    configs: &[CorrelationConfig],
    all_events: &[DeploymentEvent],
) -> Vec<CorrelatedEvent> {
    let mut claimed = vec![false; all_events.len()];
    let mut result: Vec<CorrelatedEvent> = Vec::new();

    for config in configs {
        let mut project_events: Vec<DeploymentEvent> = Vec::new();

        for (i, event) in all_events.iter().enumerate() {
            if !claimed[i] && event_matches_project(event, config) {
                project_events.push(event.clone());
                claimed[i] = true;
            }
        }

        let correlated = correlate_project_events(config, &project_events);
        result.extend(correlated);
    }

    // Remaining unmatched events become standalone
    for (i, event) in all_events.iter().enumerate() {
        if claimed[i] {
            continue;
        }

        let (github, railway, vercel) = match event.platform {
            Platform::GitHub => (Some(event.clone()), None, None),
            Platform::Railway => (None, Some(event.clone()), None),
            Platform::Vercel => (None, None, Some(event.clone())),
        };

        result.push(CorrelatedEvent {
            commit_sha: event.commit_sha.clone(),
            github,
            railway,
            vercel,
            confidence: Confidence::Unmatched,
            timestamp: event.created_at,
            is_stale: event.is_from_cache,
        });
    }

    // Sort by timestamp descending
    result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::deployment::{DeploymentStatus, EventMetadata};
    use chrono::{Duration, Utc};
    use std::collections::HashMap;

    fn gh_event(repo: &str, sha: &str, offset_secs: i64) -> DeploymentEvent {
        DeploymentEvent {
            id: format!("{repo}:run:{sha}"),
            platform: Platform::GitHub,
            status: DeploymentStatus::Success,
            commit_sha: Some(sha.into()),
            branch: Some("main".into()),
            title: Some("CI".into()),
            actor: None,
            created_at: Utc::now() + Duration::seconds(offset_secs),
            updated_at: None,
            duration_secs: Some(42),
            url: None,
            metadata: EventMetadata {
                workflow_name: Some("CI".into()),
                trigger_event: Some("push".into()),
                ..Default::default()
            },
            is_from_cache: false,
        }
    }

    fn rw_event(project_id: &str, service: &str, offset_secs: i64) -> DeploymentEvent {
        DeploymentEvent {
            id: format!("{project_id}:deploy:1"),
            platform: Platform::Railway,
            status: DeploymentStatus::Success,
            commit_sha: None,
            branch: None,
            title: None,
            actor: None,
            created_at: Utc::now() + Duration::seconds(offset_secs),
            updated_at: None,
            duration_secs: None,
            url: None,
            metadata: EventMetadata {
                service_name: Some(service.into()),
                environment_name: Some("production".into()),
                ..Default::default()
            },
            is_from_cache: false,
        }
    }

    fn vc_event(project_id: &str, sha: Option<&str>, offset_secs: i64) -> DeploymentEvent {
        DeploymentEvent {
            id: format!("{project_id}:deploy:1"),
            platform: Platform::Vercel,
            status: DeploymentStatus::Success,
            commit_sha: sha.map(Into::into),
            branch: Some("main".into()),
            title: Some("my-saas-web".into()),
            actor: None,
            created_at: Utc::now() + Duration::seconds(offset_secs),
            updated_at: None,
            duration_secs: Some(30),
            url: None,
            metadata: EventMetadata {
                deploy_target: Some("production".into()),
                ..Default::default()
            },
            is_from_cache: false,
        }
    }

    fn test_config() -> CorrelationConfig {
        CorrelationConfig {
            name: "my-saas".into(),
            github_repo: Some("myorg/my-saas".into()),
            railway_project: Some("rw-proj-1".into()),
            railway_workspace: Some("lambda-prod".into()),
            railway_environment: None,
            vercel_project: Some("prj-001".into()),
            vercel_team: Some("Lambda".into()),
            branch_mapping: HashMap::new(),
        }
    }

    #[test]
    fn exact_sha_match_github_vercel() {
        let config = test_config();
        let events = vec![
            gh_event("myorg/my-saas", "abc123", 0),
            vc_event("prj-001", Some("abc123"), 5),
        ];

        let correlated = correlate_project_events(&config, &events);
        assert_eq!(correlated.len(), 1);
        assert_eq!(correlated[0].confidence, Confidence::Exact);
        assert!(correlated[0].github.is_some());
        assert!(correlated[0].vercel.is_some());
        assert_eq!(correlated[0].commit_sha.as_deref(), Some("abc123"));
    }

    #[test]
    fn high_confidence_with_railway_mapping() {
        let config = test_config();
        let events = vec![
            gh_event("myorg/my-saas", "abc123", 0),
            rw_event("rw-proj-1", "api", 30), // within 120s window
        ];

        let correlated = correlate_project_events(&config, &events);
        assert_eq!(correlated.len(), 1);
        assert_eq!(correlated[0].confidence, Confidence::High);
        assert!(correlated[0].github.is_some());
        assert!(correlated[0].railway.is_some());
    }

    #[test]
    fn low_confidence_without_railway_mapping() {
        let mut config = test_config();
        config.railway_project = None; // no explicit mapping

        let events = vec![
            gh_event("myorg/my-saas", "abc123", 0),
            rw_event("rw-proj-1", "api", 30),
        ];

        let correlated = correlate_project_events(&config, &events);
        assert_eq!(correlated.len(), 1);
        assert_eq!(correlated[0].confidence, Confidence::Low);
    }

    #[test]
    fn unmatched_standalone_events() {
        let config = test_config();
        let events = vec![
            gh_event("myorg/my-saas", "abc123", 0),
            rw_event("rw-proj-1", "api", 300), // outside 120s window
        ];

        let correlated = correlate_project_events(&config, &events);
        assert_eq!(correlated.len(), 2);
        // Both should be Unmatched — too far apart and no SHA match
        for c in &correlated {
            assert_eq!(c.confidence, Confidence::Unmatched);
        }
    }

    #[test]
    fn all_three_platforms_match() {
        let config = test_config();
        let events = vec![
            gh_event("myorg/my-saas", "abc123", 0),
            vc_event("prj-001", Some("abc123"), 5),
            rw_event("rw-proj-1", "api", 20), // within window
        ];

        let correlated = correlate_project_events(&config, &events);
        assert_eq!(correlated.len(), 1);
        assert_eq!(correlated[0].confidence, Confidence::Exact);
        assert!(correlated[0].github.is_some());
        assert!(correlated[0].vercel.is_some());
        assert!(correlated[0].railway.is_some());
    }

    #[test]
    fn empty_events() {
        let config = test_config();
        let correlated = correlate_project_events(&config, &[]);
        assert!(correlated.is_empty());
    }

    #[test]
    fn correlate_all_multi_project() {
        let config1 = test_config();
        let config2 = CorrelationConfig {
            name: "api-core".into(),
            github_repo: Some("myorg/api-core".into()),
            railway_project: None,
            railway_workspace: None,
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: HashMap::new(),
        };

        let events = vec![
            gh_event("myorg/my-saas", "abc123", 0),
            vc_event("prj-001", Some("abc123"), 5),
            gh_event("myorg/api-core", "def456", -10),
        ];

        let correlated = correlate_all(&[config1, config2], &events);
        assert_eq!(correlated.len(), 2);

        // Should have one Exact (my-saas) and one Unmatched (api-core standalone)
        let exact = correlated
            .iter()
            .find(|c| c.confidence == Confidence::Exact);
        assert!(exact.is_some());
        assert!(exact.unwrap().github.is_some());
        assert!(exact.unwrap().vercel.is_some());

        let unmatched = correlated
            .iter()
            .find(|c| c.confidence == Confidence::Unmatched);
        assert!(unmatched.is_some());
    }

    #[test]
    fn correlate_all_unmatched_orphans() {
        // Events that don't match any config become orphan Unmatched
        let config = test_config();
        let events = vec![
            gh_event("myorg/my-saas", "abc123", 0),
            gh_event("unknown/repo", "xyz789", -20),
        ];

        let correlated = correlate_all(&[config], &events);
        assert_eq!(correlated.len(), 2);

        let orphan = correlated
            .iter()
            .find(|c| c.commit_sha.as_deref() == Some("xyz789"));
        assert!(orphan.is_some());
        assert_eq!(orphan.unwrap().confidence, Confidence::Unmatched);
    }

    #[test]
    fn event_matches_project_github() {
        let config = test_config();

        let gh = gh_event("myorg/my-saas", "abc123", 0);
        assert!(event_matches_project(&gh, &config));

        let gh_other = gh_event("myorg/other-repo", "def456", 0);
        assert!(!event_matches_project(&gh_other, &config));
    }

    #[test]
    fn event_matches_project_railway() {
        let config = test_config();

        let rw = rw_event("rw-proj-1", "api", 0);
        assert!(event_matches_project(&rw, &config));
    }

    #[test]
    fn event_matches_project_vercel() {
        let config = test_config();

        let vc = vc_event("prj-001", Some("abc123"), 0);
        assert!(event_matches_project(&vc, &config));
    }

    #[test]
    fn correlate_all_sorted_descending() {
        let config = test_config();
        let events = vec![
            gh_event("myorg/my-saas", "abc123", -100),
            gh_event("myorg/my-saas", "def456", 0),
        ];

        let correlated = correlate_all(&[config], &events);
        assert_eq!(correlated.len(), 2);
        // Most recent first
        assert!(correlated[0].timestamp >= correlated[1].timestamp);
    }
}
