//! Integration tests for the correlation engine.
//!
//! Tests `correlate_all` with realistic multi-project data across
//! GitHub, Railway, and Vercel platforms.

use chrono::{Duration, Utc};
use pulsos_core::config::types::CorrelationConfig;
use pulsos_core::correlation::{correlate_all, correlate_project_events, event_matches_project};
use pulsos_core::domain::deployment::{
    DeploymentEvent, DeploymentStatus, EventMetadata, Platform,
};
use pulsos_core::domain::project::Confidence;
use std::collections::HashMap;

fn gh_event(repo: &str, sha: &str, offset_secs: i64) -> DeploymentEvent {
    DeploymentEvent {
        id: format!("{repo}:run:{sha}"),
        platform: Platform::GitHub,
        status: DeploymentStatus::Success,
        commit_sha: Some(sha.into()),
        branch: Some("main".into()),
        title: Some("CI".into()),
        actor: Some("dev".into()),
        created_at: Utc::now() + Duration::seconds(offset_secs),
        updated_at: None,
        duration_secs: Some(45),
        url: None,
        metadata: EventMetadata {
            workflow_name: Some("CI".into()),
            trigger_event: Some("push".into()),
            ..Default::default()
        },
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
    }
}

fn vc_event(project_id: &str, sha: Option<&str>, offset_secs: i64) -> DeploymentEvent {
    DeploymentEvent {
        id: format!("{project_id}:deploy:1"),
        platform: Platform::Vercel,
        status: DeploymentStatus::Success,
        commit_sha: sha.map(Into::into),
        branch: Some("main".into()),
        title: Some("web-app".into()),
        actor: Some("dev".into()),
        created_at: Utc::now() + Duration::seconds(offset_secs),
        updated_at: None,
        duration_secs: Some(30),
        url: None,
        metadata: EventMetadata {
            deploy_target: Some("production".into()),
            ..Default::default()
        },
    }
}

fn saas_config() -> CorrelationConfig {
    CorrelationConfig {
        name: "my-saas".into(),
        github_repo: Some("myorg/my-saas".into()),
        railway_project: Some("rw-saas".into()),
        railway_workspace: Some("lambda-prod".into()),
        railway_environment: None,
        vercel_project: Some("prj-saas".into()),
        vercel_team: Some("Lambda".into()),
        branch_mapping: HashMap::new(),
    }
}

fn api_config() -> CorrelationConfig {
    CorrelationConfig {
        name: "api-core".into(),
        github_repo: Some("myorg/api-core".into()),
        railway_project: Some("rw-api".into()),
        railway_workspace: Some("lambda-prod".into()),
        railway_environment: None,
        vercel_project: None,
        vercel_team: None,
        branch_mapping: HashMap::new(),
    }
}

#[test]
fn full_three_platform_correlation() {
    let config = saas_config();
    let events = vec![
        gh_event("myorg/my-saas", "abc123", 0),
        vc_event("prj-saas", Some("abc123"), 5),
        rw_event("rw-saas", "api", 20),
    ];

    let correlated = correlate_project_events(&config, &events);
    assert_eq!(correlated.len(), 1);

    let c = &correlated[0];
    assert_eq!(c.confidence, Confidence::Exact);
    assert!(c.github.is_some());
    assert!(c.vercel.is_some());
    assert!(c.railway.is_some());
    assert_eq!(c.commit_sha.as_deref(), Some("abc123"));
}

#[test]
fn multi_project_correlate_all() {
    let configs = vec![saas_config(), api_config()];

    let events = vec![
        // my-saas: GitHub + Vercel SHA match + Railway heuristic
        gh_event("myorg/my-saas", "abc123", 0),
        vc_event("prj-saas", Some("abc123"), 5),
        rw_event("rw-saas", "api", 20),
        // api-core: GitHub + Railway heuristic
        gh_event("myorg/api-core", "def456", -100),
        rw_event("rw-api", "worker", -80),
    ];

    let correlated = correlate_all(&configs, &events);

    // Should produce 2 correlated groups
    assert_eq!(correlated.len(), 2);

    // my-saas should be Exact (SHA match)
    let saas = correlated
        .iter()
        .find(|c| c.commit_sha.as_deref() == Some("abc123"))
        .expect("should find my-saas correlation");
    assert_eq!(saas.confidence, Confidence::Exact);
    assert!(saas.github.is_some());
    assert!(saas.vercel.is_some());
    assert!(saas.railway.is_some());

    // api-core should be High (has explicit railway mapping)
    let api = correlated
        .iter()
        .find(|c| c.commit_sha.as_deref() == Some("def456"))
        .expect("should find api-core correlation");
    assert_eq!(api.confidence, Confidence::High);
    assert!(api.github.is_some());
    assert!(api.railway.is_some());
}

#[test]
fn orphan_events_become_unmatched() {
    let configs = vec![saas_config()];

    let events = vec![
        gh_event("myorg/my-saas", "abc123", 0),
        // This event doesn't match any config
        gh_event("unknown/repo", "zzz999", -50),
    ];

    let correlated = correlate_all(&configs, &events);
    assert_eq!(correlated.len(), 2);

    let orphan = correlated
        .iter()
        .find(|c| c.commit_sha.as_deref() == Some("zzz999"))
        .expect("orphan should exist");
    assert_eq!(orphan.confidence, Confidence::Unmatched);
}

#[test]
fn results_sorted_descending_by_timestamp() {
    let configs = vec![saas_config()];

    let events = vec![
        gh_event("myorg/my-saas", "old", -300),
        gh_event("myorg/my-saas", "new", 0),
        gh_event("myorg/my-saas", "mid", -100),
    ];

    let correlated = correlate_all(&configs, &events);
    assert_eq!(correlated.len(), 3);

    // Check descending order
    for window in correlated.windows(2) {
        assert!(window[0].timestamp >= window[1].timestamp);
    }
}

#[test]
fn event_matches_project_across_platforms() {
    let config = saas_config();

    // GitHub match
    let gh = gh_event("myorg/my-saas", "abc", 0);
    assert!(event_matches_project(&gh, &config));

    // GitHub non-match
    let gh_other = gh_event("myorg/other", "abc", 0);
    assert!(!event_matches_project(&gh_other, &config));

    // Railway match
    let rw = rw_event("rw-saas", "api", 0);
    assert!(event_matches_project(&rw, &config));

    // Vercel match
    let vc = vc_event("prj-saas", Some("abc"), 0);
    assert!(event_matches_project(&vc, &config));
}

#[test]
fn railway_outside_window_not_matched() {
    let config = saas_config();
    let events = vec![
        gh_event("myorg/my-saas", "abc123", 0),
        rw_event("rw-saas", "api", 300), // > 120s window
    ];

    let correlated = correlate_project_events(&config, &events);
    assert_eq!(correlated.len(), 2);

    // Both should be Unmatched since they're too far apart
    for c in &correlated {
        assert_eq!(c.confidence, Confidence::Unmatched);
    }
}

#[test]
fn empty_configs_and_events() {
    let correlated = correlate_all(&[], &[]);
    assert!(correlated.is_empty());
}
