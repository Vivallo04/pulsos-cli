use pulsos_core::auth::credential_store::{CredentialStore, InMemoryStore};
use pulsos_core::auth::resolve::{TokenResolver, TokenSource};
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::types::TokenDetectionConfig;
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::PlatformAdapter;
use pulsos_test::mock_server::MockGitHub;
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;

// ── Token Resolver Integration ──

#[tokio::test]
async fn resolve_token_from_keyring_and_validate_github() {
    let mock = MockGitHub::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());

    let store = Arc::new(InMemoryStore::new());
    store
        .set(&PlatformKind::GitHub, "test-github-token")
        .unwrap();

    let resolver = TokenResolver::new(
        store,
        TokenDetectionConfig {
            detect_env_vars: false,
            detect_gh_cli: false,
            detect_railway_cli: false,
            detect_vercel_cli: false,
        },
    );

    // Resolve from keyring
    let (token, source) = resolver
        .resolve_with_source(&PlatformKind::GitHub)
        .expect("Should resolve token from keyring");
    assert_eq!(token.expose_secret(), "test-github-token");
    assert_eq!(source, TokenSource::Keyring);

    // Validate via mock API
    let client = GitHubClient::new_with_base_url(token, mock.url(), cache.clone());
    let status = client.validate_auth().await.unwrap();
    assert!(status.valid);
    assert_eq!(status.identity, "@vivallo");
}

#[tokio::test]
async fn validate_github_token_with_scopes() {
    let mock = MockGitHub::start().await;
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(CacheStore::open(&dir.path().join("cache")).unwrap());

    // The mock returns x-oauth-scopes: "repo, read:org"
    // validate_token should return success with no warnings
    let client = GitHubClient::new_with_base_url(
        SecretString::new("test-github-token".into()),
        mock.url(),
        cache.clone(),
    );
    let status = client.validate_auth().await.unwrap();

    assert!(status.valid);
    assert!(status.scopes.contains(&"repo".to_string()));
    assert!(status.scopes.contains(&"read:org".to_string()));
    // No warnings because both required and recommended scopes are present
    assert!(status.warnings.is_empty());
}

#[tokio::test]
async fn resolve_railway_token_from_keyring() {
    let store = Arc::new(InMemoryStore::new());
    store
        .set(&PlatformKind::Railway, "test-railway-token")
        .unwrap();

    let resolver = TokenResolver::new(
        store,
        TokenDetectionConfig {
            detect_env_vars: false,
            detect_gh_cli: false,
            detect_railway_cli: false,
            detect_vercel_cli: false,
        },
    );

    let (token, source) = resolver
        .resolve_with_source(&PlatformKind::Railway)
        .expect("Should resolve Railway token");
    assert_eq!(token.expose_secret(), "test-railway-token");
    assert_eq!(source, TokenSource::Keyring);
}

#[tokio::test]
async fn resolver_returns_none_when_no_sources() {
    let store = Arc::new(InMemoryStore::new());
    let resolver = TokenResolver::new(
        store,
        TokenDetectionConfig {
            detect_env_vars: false,
            detect_gh_cli: false,
            detect_railway_cli: false,
            detect_vercel_cli: false,
        },
    );

    assert!(resolver.resolve(&PlatformKind::GitHub).is_none());
    assert!(resolver.resolve(&PlatformKind::Railway).is_none());
    assert!(resolver.resolve(&PlatformKind::Vercel).is_none());
}

#[tokio::test]
async fn multiple_platforms_resolved_independently() {
    let store = Arc::new(InMemoryStore::new());
    store.set(&PlatformKind::GitHub, "gh-token").unwrap();
    store.set(&PlatformKind::Vercel, "vc-token").unwrap();
    // Railway deliberately not set

    let resolver = TokenResolver::new(
        store,
        TokenDetectionConfig {
            detect_env_vars: false,
            detect_gh_cli: false,
            detect_railway_cli: false,
            detect_vercel_cli: false,
        },
    );

    assert_eq!(
        resolver
            .resolve(&PlatformKind::GitHub)
            .unwrap()
            .expose_secret(),
        "gh-token"
    );
    assert!(resolver.resolve(&PlatformKind::Railway).is_none());
    assert_eq!(
        resolver
            .resolve(&PlatformKind::Vercel)
            .unwrap()
            .expose_secret(),
        "vc-token"
    );
}

// ── Credential Store Integration ──

#[test]
fn in_memory_store_lifecycle() {
    use pulsos_core::auth::credential_store::CredentialStore;

    let store = InMemoryStore::new();

    // Initially empty
    assert!(store.get(&PlatformKind::GitHub).unwrap().is_none());

    // Set and get
    store.set(&PlatformKind::GitHub, "token1").unwrap();
    assert_eq!(
        store
            .get(&PlatformKind::GitHub)
            .unwrap()
            .unwrap()
            .expose_secret(),
        "token1"
    );

    // Overwrite
    store.set(&PlatformKind::GitHub, "token2").unwrap();
    assert_eq!(
        store
            .get(&PlatformKind::GitHub)
            .unwrap()
            .unwrap()
            .expose_secret(),
        "token2"
    );

    // Delete
    store.delete(&PlatformKind::GitHub).unwrap();
    assert!(store.get(&PlatformKind::GitHub).unwrap().is_none());

    // Delete again is a no-op
    store.delete(&PlatformKind::GitHub).unwrap();
}

// ── Token Detection Integration ──

#[test]
fn detect_gh_token_from_temp_file() {
    use pulsos_core::auth::detect;
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let hosts_path = dir.path().join("hosts.yml");
    let mut file = std::fs::File::create(&hosts_path).unwrap();
    writeln!(
        file,
        "github.com:\n    oauth_token: gho_integration_test\n    user: testuser"
    )
    .unwrap();

    // Set GH_CONFIG_DIR to point to our temp dir
    unsafe { std::env::set_var("GH_CONFIG_DIR", dir.path()) };

    let token = detect::detect_gh_token();
    assert_eq!(token, Some("gho_integration_test".to_string()));

    unsafe { std::env::remove_var("GH_CONFIG_DIR") };
}
