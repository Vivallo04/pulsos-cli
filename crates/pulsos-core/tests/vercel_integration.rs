use pulsos_core::cache::store::CacheStore;
use pulsos_core::domain::deployment::{DeploymentStatus, Platform};
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::{PlatformAdapter, TrackedResource};
use pulsos_test::mock_server::MockVercel;
use secrecy::SecretString;
use std::sync::Arc;

#[tokio::test]
async fn fetch_events_returns_deployments() {
    let mock = MockVercel::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = VercelClient::new_with_base_url(
        SecretString::new("test-vercel-token".into()),
        mock.url(),
        cache,
    )
    .unwrap();

    let tracked = vec![TrackedResource {
        platform_id: "my-saas-web".into(),
        display_name: "my-saas-web".into(),
        group: Some("Lambda".into()),
    }];

    let events = client.fetch_events(&tracked).await.unwrap();
    assert_eq!(events.len(), 2);

    // All events should be Vercel
    for event in &events {
        assert_eq!(event.platform, Platform::Vercel);
    }

    // First should have commit SHA from meta (for correlation)
    let ready_event = events
        .iter()
        .find(|e| e.status == DeploymentStatus::Success);
    assert!(ready_event.is_some());
    let ready = ready_event.unwrap();
    assert_eq!(ready.commit_sha.as_deref(), Some("abc123def456789"));
    assert_eq!(ready.branch.as_deref(), Some("main"));
    assert_eq!(ready.actor.as_deref(), Some("vivallo"));
}

#[tokio::test]
async fn validate_auth_succeeds() {
    let mock = MockVercel::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = VercelClient::new_with_base_url(
        SecretString::new("test-vercel-token".into()),
        mock.url(),
        cache,
    )
    .unwrap();

    let status = client.validate_auth().await.unwrap();
    assert!(status.valid);
    assert_eq!(status.identity, "vivallo");
}

#[tokio::test]
async fn discover_returns_projects() {
    let mock = MockVercel::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = VercelClient::new_with_base_url(
        SecretString::new("test-vercel-token".into()),
        mock.url(),
        cache,
    )
    .unwrap();

    let resources = client.discover().await.unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].display_name, "my-saas-web");
}
