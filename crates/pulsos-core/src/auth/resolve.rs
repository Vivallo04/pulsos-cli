//! Centralized token resolution with priority chain:
//!
//! 1. Environment variable
//! 2. OS keyring (via CredentialStore)
//! 3. CLI config detection

use crate::auth::credential_store::CredentialStore;
use crate::auth::detect;
use crate::auth::PlatformKind;
use crate::config::types::TokenDetectionConfig;
use secrecy::SecretString;
use std::fmt;
use std::sync::Arc;

/// Where a token was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenSource {
    /// Found in an environment variable (includes var name).
    EnvVar(String),
    /// Found in the OS keyring.
    Keyring,
    /// Found in a CLI tool's config file (includes tool name).
    CliConfig(String),
}

impl fmt::Display for TokenSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvVar(name) => write!(f, "{name}"),
            Self::Keyring => write!(f, "keyring"),
            Self::CliConfig(tool) => write!(f, "{tool} CLI config"),
        }
    }
}

/// Resolves tokens for platforms using a priority chain.
pub struct TokenResolver {
    store: Arc<dyn CredentialStore>,
    detection_config: TokenDetectionConfig,
}

impl TokenResolver {
    pub fn new(store: Arc<dyn CredentialStore>, detection_config: TokenDetectionConfig) -> Self {
        Self {
            store,
            detection_config,
        }
    }

    /// Resolve a token using the priority chain. Returns the first found token.
    pub fn resolve(&self, platform: &PlatformKind) -> Option<SecretString> {
        self.resolve_with_source(platform).map(|(token, _)| token)
    }

    /// Resolve a token and report where it came from.
    pub fn resolve_with_source(
        &self,
        platform: &PlatformKind,
    ) -> Option<(SecretString, TokenSource)> {
        // 1. Environment variables
        if self.detection_config.detect_env_vars {
            if let Some(result) = self.try_env_var(platform) {
                return Some(result);
            }
        }

        // 2. OS keyring
        if let Some(result) = self.try_keyring(platform) {
            return Some(result);
        }

        // 3. CLI config detection
        if let Some(result) = self.try_cli_config(platform) {
            return Some(result);
        }

        None
    }

    fn try_env_var(&self, platform: &PlatformKind) -> Option<(SecretString, TokenSource)> {
        for var_name in platform.env_var_names() {
            if let Ok(token) = std::env::var(var_name) {
                if !token.is_empty() {
                    return Some((
                        SecretString::new(token),
                        TokenSource::EnvVar((*var_name).to_string()),
                    ));
                }
            }
        }
        None
    }

    fn try_keyring(&self, platform: &PlatformKind) -> Option<(SecretString, TokenSource)> {
        match self.store.get(platform) {
            Ok(Some(token)) => Some((token, TokenSource::Keyring)),
            Ok(None) => None,
            Err(e) => {
                tracing::debug!(
                    platform = %platform.display_name(),
                    error = %e,
                    "Failed to read from keyring"
                );
                None
            }
        }
    }

    fn try_cli_config(&self, platform: &PlatformKind) -> Option<(SecretString, TokenSource)> {
        match platform {
            PlatformKind::GitHub => {
                if self.detection_config.detect_gh_cli {
                    detect::detect_gh_token().map(|t| {
                        (
                            SecretString::new(t),
                            TokenSource::CliConfig("gh".to_string()),
                        )
                    })
                } else {
                    None
                }
            }
            PlatformKind::Railway => {
                if self.detection_config.detect_railway_cli {
                    detect::detect_railway_token().map(|t| {
                        (
                            SecretString::new(t),
                            TokenSource::CliConfig("railway".to_string()),
                        )
                    })
                } else {
                    None
                }
            }
            PlatformKind::Vercel => {
                if self.detection_config.detect_vercel_cli {
                    detect::detect_vercel_token().map(|t| {
                        (
                            SecretString::new(t),
                            TokenSource::CliConfig("vercel".to_string()),
                        )
                    })
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::credential_store::InMemoryStore;
    use secrecy::ExposeSecret;

    fn test_resolver_no_detection() -> TokenResolver {
        TokenResolver::new(
            Arc::new(InMemoryStore::new()),
            TokenDetectionConfig {
                detect_gh_cli: false,
                detect_railway_cli: false,
                detect_vercel_cli: false,
                detect_env_vars: false,
            },
        )
    }

    #[test]
    fn resolve_returns_none_when_no_source() {
        let resolver = test_resolver_no_detection();
        assert!(resolver.resolve(&PlatformKind::GitHub).is_none());
        assert!(resolver.resolve(&PlatformKind::Railway).is_none());
        assert!(resolver.resolve(&PlatformKind::Vercel).is_none());
    }

    #[test]
    fn resolve_from_keyring() {
        let store = Arc::new(InMemoryStore::new());
        store.set(&PlatformKind::GitHub, "keyring_token").unwrap();

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
            .resolve_with_source(&PlatformKind::GitHub)
            .expect("should resolve");
        assert_eq!(token.expose_secret(), "keyring_token");
        assert_eq!(source, TokenSource::Keyring);
    }

    #[test]
    fn keyring_wins_when_env_detection_disabled() {
        let store = Arc::new(InMemoryStore::new());
        store.set(&PlatformKind::GitHub, "keyring_token").unwrap();

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
            .resolve_with_source(&PlatformKind::GitHub)
            .expect("should resolve");
        assert_eq!(token.expose_secret(), "keyring_token");
        assert_eq!(source, TokenSource::Keyring);
    }

    #[test]
    fn env_var_takes_priority_over_keyring() {
        let var_name = PlatformKind::GitHub.env_var_names()[0];

        let store = Arc::new(InMemoryStore::new());
        store.set(&PlatformKind::GitHub, "keyring_token").unwrap();

        let resolver = TokenResolver::new(
            store,
            TokenDetectionConfig {
                detect_env_vars: true,
                detect_gh_cli: false,
                detect_railway_cli: false,
                detect_vercel_cli: false,
            },
        );

        temp_env::with_var(var_name, Some("env_priority_token"), || {
            let (token, source) = resolver
                .resolve_with_source(&PlatformKind::GitHub)
                .expect("should resolve");
            assert_eq!(token.expose_secret(), "env_priority_token");
            assert!(
                matches!(source, TokenSource::EnvVar(_)),
                "expected EnvVar source, got {source:?}"
            );
        });
    }

    #[test]
    fn resolve_with_env_var_disabled() {
        let resolver = TokenResolver::new(
            Arc::new(InMemoryStore::new()),
            TokenDetectionConfig {
                detect_env_vars: false,
                detect_gh_cli: false,
                detect_railway_cli: false,
                detect_vercel_cli: false,
            },
        );

        // Even if GITHUB_TOKEN is set in the environment, it should be ignored
        assert!(resolver.resolve(&PlatformKind::GitHub).is_none());
    }

    #[test]
    fn token_source_display() {
        assert_eq!(
            TokenSource::EnvVar("GITHUB_TOKEN".into()).to_string(),
            "GITHUB_TOKEN"
        );
        assert_eq!(TokenSource::Keyring.to_string(), "keyring");
        assert_eq!(
            TokenSource::CliConfig("gh".into()).to_string(),
            "gh CLI config"
        );
    }

    #[test]
    fn resolve_all_platforms_from_keyring() {
        let store = Arc::new(InMemoryStore::new());
        store.set(&PlatformKind::GitHub, "gh_tok").unwrap();
        store.set(&PlatformKind::Railway, "rw_tok").unwrap();
        store.set(&PlatformKind::Vercel, "vc_tok").unwrap();

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
            "gh_tok"
        );
        assert_eq!(
            resolver
                .resolve(&PlatformKind::Railway)
                .unwrap()
                .expose_secret(),
            "rw_tok"
        );
        assert_eq!(
            resolver
                .resolve(&PlatformKind::Vercel)
                .unwrap()
                .expose_secret(),
            "vc_tok"
        );
    }
}
