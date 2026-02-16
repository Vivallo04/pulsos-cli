use pulsos_core::cache::store::CacheStore;
use pulsos_core::domain::deployment::{DeploymentStatus, Platform};
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::{PlatformAdapter, TrackedResource};
use pulsos_test::mock_server::MockRailway;
use std::sync::Arc;

#[tokio::test]
async fn fetch_events_returns_deployments() {
    let mock = MockRailway::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = RailwayClient::new_with_base_url(
        "test-railway-token".into(),
        format!("{}/graphql/v2", mock.url()),
        cache,
    );

    let tracked = vec![TrackedResource {
        platform_id: "proj-001:svc-001:env-001".into(),
        display_name: "my-saas-api".into(),
        group: Some("lambda-prod".into()),
    }];

    let events = client.fetch_events(&tracked).await.unwrap();
    assert_eq!(events.len(), 2);

    // Check platforms
    for event in &events {
        assert_eq!(event.platform, Platform::Railway);
    }

    // Should contain both SUCCESS and BUILDING deployments
    let statuses: Vec<_> = events.iter().map(|e| &e.status).collect();
    assert!(statuses.contains(&&DeploymentStatus::Success));
    assert!(statuses.contains(&&DeploymentStatus::InProgress));
}

#[tokio::test]
async fn validate_auth_succeeds() {
    let mock = MockRailway::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = RailwayClient::new_with_base_url(
        "test-railway-token".into(),
        format!("{}/graphql/v2", mock.url()),
        cache,
    );

    let status = client.validate_auth().await.unwrap();
    assert!(status.valid);
    assert_eq!(status.identity, "test@lambda.co");
}
