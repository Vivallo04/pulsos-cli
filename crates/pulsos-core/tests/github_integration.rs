use pulsos_core::cache::store::CacheStore;
use pulsos_core::domain::deployment::{DeploymentStatus, Platform};
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::{PlatformAdapter, TrackedResource};
use pulsos_test::mock_server::MockGitHub;
use secrecy::SecretString;
use std::sync::Arc;
use wiremock::matchers::{header, method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_token() -> SecretString {
    SecretString::new("test-github-token".into())
}

#[tokio::test]
async fn fetch_events_returns_runs() {
    let mock = MockGitHub::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = GitHubClient::new_with_base_url(test_token(), mock.url(), cache).unwrap();

    let tracked = vec![TrackedResource {
        platform_id: "myorg/my-saas".into(),
        display_name: "my-saas".into(),
        group: None,
    }];

    let events = client.fetch_events(&tracked).await.unwrap();
    assert_eq!(events.len(), 2);

    // Events should be sorted by created_at descending
    assert!(events[0].created_at >= events[1].created_at);

    // Check first event
    let first = &events[0];
    assert_eq!(first.platform, Platform::GitHub);
    assert_eq!(first.status, DeploymentStatus::Success);
    assert_eq!(first.commit_sha.as_deref(), Some("abc123def456789"));
    assert_eq!(first.branch.as_deref(), Some("main"));
    assert_eq!(first.actor.as_deref(), Some("vivallo"));
    assert!(first.duration_secs.is_some());

    // Check second event
    let second = &events[1];
    assert_eq!(second.status, DeploymentStatus::InProgress);

    let first_job = first.metadata.job_details.first().unwrap();
    assert_eq!(first_job.job_id, Some(700001));
    assert!(first_job.html_url.is_some());
    let first_step = first_job.steps.first().unwrap();
    assert!(first_step.started_at.is_some());
    assert!(first_step.completed_at.is_some());
}

#[tokio::test]
async fn validate_auth_succeeds() {
    let mock = MockGitHub::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = GitHubClient::new_with_base_url(test_token(), mock.url(), cache).unwrap();

    let status = client.validate_auth().await.unwrap();
    assert!(status.valid);
    assert_eq!(status.identity, "@vivallo");
    assert!(status.scopes.contains(&"repo".to_string()));
}

#[tokio::test]
async fn discover_returns_repos() {
    let mock = MockGitHub::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = GitHubClient::new_with_base_url(test_token(), mock.url(), cache).unwrap();

    let resources = client.discover().await.unwrap();
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].platform_id, "myorg/my-saas");
    assert_eq!(resources[0].display_name, "my-saas");
    assert_eq!(resources[0].group, "myorg");
}

#[tokio::test]
async fn rate_limit_updated_from_headers() {
    let mock = MockGitHub::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = GitHubClient::new_with_base_url(test_token(), mock.url(), cache).unwrap();

    // Before any requests, rate limit is unknown
    let rl = client.rate_limit_status().await.unwrap();
    assert_eq!(rl.limit, 0);
    assert_eq!(rl.remaining, 0);

    // After a request, rate limit should be updated from headers
    let _ = client.validate_auth().await.unwrap();
    let rl = client.rate_limit_status().await.unwrap();
    assert_eq!(rl.limit, 5000);
    assert_eq!(rl.remaining, 4999);
}

#[tokio::test]
async fn fetch_job_log_returns_text() {
    let mock = MockGitHub::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = GitHubClient::new_with_base_url(test_token(), mock.url(), cache).unwrap();

    let log = client.fetch_job_log("myorg/my-saas", 700001).await.unwrap();
    assert!(log.contains("Checkout"));
    assert!(log.contains("Build"));
}

#[tokio::test]
async fn fetch_job_log_auth_failure_is_mapped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/.+/.+/actions/jobs/\d+/logs$"))
        .and(header("Authorization", "Bearer test-github-token"))
        .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());
    let client = GitHubClient::new_with_base_url(test_token(), server.uri(), cache).unwrap();

    let err = client
        .fetch_job_log("myorg/my-saas", 700001)
        .await
        .unwrap_err();
    let msg = err.user_message();
    assert!(msg.contains("Authentication failed for GitHub"));
}
