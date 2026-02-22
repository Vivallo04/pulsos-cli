use pulsos_core::config::types::{CorrelationConfig, PulsosConfig};
use pulsos_core::config::{load_config, save_config};
use pulsos_core::platform::DiscoveredResource;
use pulsos_core::sync::correlate::{
    build_correlations, candidate_to_config, DiscoveryResults, MatchConfidence,
};
use pulsos_core::sync::merge::{merge_correlations, populate_platform_sections};
use std::collections::HashMap;

// ── Correlation engine integration tests ──

#[test]
fn discover_correlate_save_load_roundtrip() {
    let discovery = DiscoveryResults {
        github: vec![DiscoveredResource {
            platform_id: "myorg/my-saas".into(),
            display_name: "my-saas".into(),
            group: "myorg".into(),
            group_type: "org".into(),
            archived: false,
            disabled: false,
        }],
        railway: vec![DiscoveredResource {
            platform_id: "proj-001:svc-001:env-001".into(),
            display_name: "my-saas-api / api / production".into(),
            group: "lambda-prod".into(),
            group_type: "workspace".into(),
            archived: false,
            disabled: false,
        }],
        vercel: vec![(
            DiscoveredResource {
                platform_id: "prj-001".into(),
                display_name: "my-saas-web".into(),
                group: "Lambda".into(),
                group_type: "team".into(),
                archived: false,
                disabled: false,
            },
            Some("myorg/my-saas".into()),
        )],
    };

    let candidates = build_correlations(&discovery);
    assert!(
        !candidates.is_empty(),
        "Should produce at least one candidate"
    );

    // There should be a single "my-saas" correlation grouping all 3 platforms.
    let main = candidates.iter().find(|c| c.name == "my-saas");
    assert!(main.is_some(), "Should have a 'my-saas' correlation");
    let main = main.unwrap();
    assert!(main.github.is_some(), "Should include GitHub");
    assert!(main.railway.is_some(), "Should include Railway");
    assert!(main.vercel.is_some(), "Should include Vercel");

    // Convert and save
    let correlations: Vec<CorrelationConfig> = candidates.iter().map(candidate_to_config).collect();
    let (mut config, added, _updated) = merge_correlations(&PulsosConfig::default(), correlations);
    populate_platform_sections(&mut config);

    assert!(added > 0, "Should have added correlations");
    assert!(
        !config.correlations.is_empty(),
        "Config should have correlations"
    );

    // Save to temp file and reload
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    save_config(&config, Some(&path)).unwrap();

    let reloaded = load_config(Some(&path)).unwrap();
    assert_eq!(reloaded.correlations.len(), config.correlations.len());

    let reloaded_main = reloaded.correlations.iter().find(|c| c.name == "my-saas");
    assert!(reloaded_main.is_some());
    let reloaded_main = reloaded_main.unwrap();
    assert!(reloaded_main.github_repo.is_some());
    assert!(reloaded_main.railway_project.is_some());
    assert!(reloaded_main.vercel_project.is_some());
}

#[test]
fn vercel_linked_repo_creates_exact_match() {
    let discovery = DiscoveryResults {
        github: vec![DiscoveredResource {
            platform_id: "myorg/my-saas".into(),
            display_name: "my-saas".into(),
            group: "myorg".into(),
            group_type: "org".into(),
            archived: false,
            disabled: false,
        }],
        railway: vec![],
        vercel: vec![(
            DiscoveredResource {
                platform_id: "prj-001".into(),
                display_name: "my-saas-web".into(),
                group: "Lambda".into(),
                group_type: "team".into(),
                archived: false,
                disabled: false,
            },
            Some("myorg/my-saas".into()),
        )],
    };

    let candidates = build_correlations(&discovery);

    // The Vercel project linked to "myorg/my-saas" should match the GitHub repo exactly.
    let linked = candidates
        .iter()
        .find(|c| c.github.is_some() && c.vercel.is_some());
    assert!(linked.is_some(), "Should have a linked correlation");
    let linked = linked.unwrap();
    assert_eq!(linked.confidence, MatchConfidence::LinkedRepo);
    assert_eq!(linked.name, "my-saas");
}

#[test]
fn name_stem_matching_across_platforms() {
    let discovery = DiscoveryResults {
        github: vec![DiscoveredResource {
            platform_id: "myorg/my-saas".into(),
            display_name: "my-saas".into(),
            group: "myorg".into(),
            group_type: "org".into(),
            archived: false,
            disabled: false,
        }],
        railway: vec![DiscoveredResource {
            platform_id: "proj-001:svc-001:env-001".into(),
            display_name: "my-saas-api / api / production".into(),
            group: "lambda-prod".into(),
            group_type: "workspace".into(),
            archived: false,
            disabled: false,
        }],
        vercel: vec![(
            DiscoveredResource {
                platform_id: "prj-001".into(),
                display_name: "my-saas-web".into(),
                group: "Lambda".into(),
                group_type: "team".into(),
                archived: false,
                disabled: false,
            },
            None, // No linked repo — forces name stem matching
        )],
    };

    let candidates = build_correlations(&discovery);

    // All three should be grouped under "my-saas" via stem matching.
    let main = candidates.iter().find(|c| c.name == "my-saas");
    assert!(main.is_some(), "Should have a 'my-saas' correlation");
    let main = main.unwrap();
    assert!(main.github.is_some(), "Should include GitHub");
    assert!(main.railway.is_some(), "Should include Railway");
    assert!(main.vercel.is_some(), "Should include Vercel");
    assert_eq!(main.confidence, MatchConfidence::ExactStem);
}

#[test]
fn unmatched_resources_standalone() {
    let discovery = DiscoveryResults {
        github: vec![DiscoveredResource {
            platform_id: "myorg/api-core".into(),
            display_name: "api-core".into(),
            group: "myorg".into(),
            group_type: "org".into(),
            archived: false,
            disabled: false,
        }],
        railway: vec![],
        vercel: vec![],
    };

    let candidates = build_correlations(&discovery);
    assert_eq!(candidates.len(), 1);

    let c = &candidates[0];
    assert_eq!(c.name, "api-core");
    assert_eq!(c.confidence, MatchConfidence::Unmatched);
    assert!(c.github.is_some());
    assert!(c.railway.is_none());
    assert!(c.vercel.is_none());
}

#[test]
fn merge_preserves_branch_mappings() {
    let mut existing = PulsosConfig::default();
    existing.correlations.push(CorrelationConfig {
        name: "my-saas".into(),
        github_repo: Some("myorg/my-saas".into()),
        railway_project: None,
        railway_workspace: None,
        railway_environment: None,
        vercel_project: None,
        vercel_team: None,
        branch_mapping: {
            let mut m = HashMap::new();
            m.insert("main".into(), "production".into());
            m.insert("develop".into(), "staging".into());
            m
        },
    });

    let new_correlations = vec![CorrelationConfig {
        name: "my-saas".into(),
        github_repo: Some("myorg/my-saas".into()),
        railway_project: Some("proj-001:svc-001:env-001".into()),
        railway_workspace: Some("lambda-prod".into()),
        railway_environment: None,
        vercel_project: Some("prj-001".into()),
        vercel_team: Some("Lambda".into()),
        branch_mapping: HashMap::new(),
    }];

    let (config, _added, updated) = merge_correlations(&existing, new_correlations);
    assert_eq!(updated, 1, "Should have updated 1 correlation");

    let corr = config.correlations.iter().find(|c| c.name == "my-saas");
    assert!(corr.is_some());
    let corr = corr.unwrap();

    // Original branch_mapping should be preserved
    assert_eq!(
        corr.branch_mapping.get("main"),
        Some(&"production".to_string())
    );
    assert_eq!(
        corr.branch_mapping.get("develop"),
        Some(&"staging".to_string())
    );

    // New platform fields should be set
    assert!(corr.railway_project.is_some());
    assert!(corr.vercel_project.is_some());
}

// ── Mock server integration tests ──

use pulsos_core::cache::store::CacheStore;
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::PlatformAdapter;
use pulsos_test::mock_server::{MockRailway, MockVercel};
use secrecy::SecretString;
use std::sync::Arc;

#[tokio::test]
async fn railway_discover_returns_services() {
    let mock = MockRailway::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = RailwayClient::new_with_base_url(
        SecretString::new("test-railway-token".into()),
        format!("{}/graphql/v2", mock.url()),
        cache,
    )
    .unwrap();

    let resources = client.discover().await.unwrap();
    assert!(!resources.is_empty(), "Should discover Railway services");

    // Based on fixtures: 1 project, 1 service, 1 environment → 1 resource
    assert_eq!(resources.len(), 1);

    let r = &resources[0];
    // platform_id should be "projectId:serviceId:environmentId"
    assert!(
        r.platform_id.contains(':'),
        "Platform ID should be composite (proj:svc:env)"
    );
    assert_eq!(r.platform_id, "proj-001:svc-001:env-001");
    assert!(r.display_name.contains("my-saas-api"));
    assert_eq!(r.group, "lambda-prod");
    assert_eq!(r.group_type, "workspace");
}

#[tokio::test]
async fn vercel_discover_with_links_returns_linked_repo() {
    let mock = MockVercel::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = VercelClient::new_with_base_url(
        SecretString::new("test-vercel-token".into()),
        mock.url(),
        cache,
    )
    .unwrap();

    let results = client.discover_with_links().await.unwrap();
    assert_eq!(results.len(), 1);

    let (resource, linked_repo) = &results[0];
    assert_eq!(resource.display_name, "my-saas-web");
    assert_eq!(resource.group, "Lambda");
    assert_eq!(linked_repo.as_deref(), Some("myorg/my-saas"));
}

#[tokio::test]
async fn full_discovery_and_correlation_with_mock_servers() {
    // Start all mock servers
    let github_mock = pulsos_test::mock_server::MockGitHub::start().await;
    let railway_mock = MockRailway::start().await;
    let vercel_mock = MockVercel::start().await;

    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());

    // Discover from all platforms
    let github_client = pulsos_core::platform::github::client::GitHubClient::new_with_base_url(
        SecretString::new("test-github-token".into()),
        github_mock.url(),
        cache.clone(),
    )
    .unwrap();
    let railway_client = RailwayClient::new_with_base_url(
        SecretString::new("test-railway-token".into()),
        format!("{}/graphql/v2", railway_mock.url()),
        cache.clone(),
    )
    .unwrap();
    let vercel_client = VercelClient::new_with_base_url(
        SecretString::new("test-vercel-token".into()),
        vercel_mock.url(),
        cache.clone(),
    )
    .unwrap();

    let github_resources = github_client.discover().await.unwrap();
    let railway_resources = railway_client.discover().await.unwrap();
    let vercel_resources = vercel_client.discover_with_links().await.unwrap();

    assert!(!github_resources.is_empty(), "GitHub should discover repos");
    assert!(
        !railway_resources.is_empty(),
        "Railway should discover services"
    );
    assert!(
        !vercel_resources.is_empty(),
        "Vercel should discover projects"
    );

    // Build correlations
    let discovery = DiscoveryResults {
        github: github_resources,
        railway: railway_resources,
        vercel: vercel_resources,
    };

    let candidates = build_correlations(&discovery);
    assert!(!candidates.is_empty());

    // There should be a "my-saas" correlation that groups all 3 platforms.
    let main = candidates.iter().find(|c| c.name == "my-saas");
    assert!(main.is_some(), "Should find 'my-saas' correlation");
    let main = main.unwrap();
    assert!(main.github.is_some());
    assert!(main.railway.is_some());
    assert!(main.vercel.is_some());

    // "api-core" should be standalone (no Railway/Vercel match).
    let standalone = candidates.iter().find(|c| c.name == "api-core");
    assert!(standalone.is_some(), "Should find 'api-core' standalone");
    let standalone = standalone.unwrap();
    assert!(standalone.github.is_some());
    assert!(standalone.railway.is_none());
    assert!(standalone.vercel.is_none());
    assert_eq!(standalone.confidence, MatchConfidence::Unmatched);

    // Convert to config and save
    let configs: Vec<CorrelationConfig> = candidates.iter().map(candidate_to_config).collect();
    let (mut config, added, _) = merge_correlations(&PulsosConfig::default(), configs);
    populate_platform_sections(&mut config);

    assert!(added >= 2, "Should have added at least 2 correlations");

    // Verify platform sections were populated
    assert!(!config.github.organizations.is_empty());
    assert!(!config.railway.workspaces.is_empty());
    assert!(!config.vercel.teams.is_empty());

    // Save and reload
    let config_path = dir.path().join("config.toml");
    save_config(&config, Some(&config_path)).unwrap();
    let reloaded = load_config(Some(&config_path)).unwrap();
    assert_eq!(reloaded.correlations.len(), config.correlations.len());
}
