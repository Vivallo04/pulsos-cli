//! Token validation and scope checking.
//!
//! Validates tokens against their respective platform APIs and checks
//! for required/dangerous scopes.

use crate::auth::PlatformKind;
use crate::cache::store::CacheStore;
use crate::error::PulsosError;
use crate::platform::github::client::GitHubClient;
use crate::platform::railway::client::RailwayClient;
use crate::platform::vercel::client::VercelClient;
use crate::platform::{AuthStatus, PlatformAdapter};
use secrecy::SecretString;
use std::sync::Arc;

/// Validate a token against the platform API and check scopes.
///
/// Returns an `AuthStatus` with any warnings populated (e.g., missing
/// required scopes, dangerous scopes present).
pub async fn validate_token(
    platform: &PlatformKind,
    token: SecretString,
    cache: &Arc<CacheStore>,
) -> Result<AuthStatus, PulsosError> {
    let mut status = match platform {
        PlatformKind::GitHub => {
            let client = GitHubClient::new(token, cache.clone());
            client.validate_auth().await?
        }
        PlatformKind::Railway => {
            let client = RailwayClient::new(token, cache.clone());
            client.validate_auth().await?
        }
        PlatformKind::Vercel => {
            let client = VercelClient::new(token, cache.clone());
            client.validate_auth().await?
        }
    };

    // Platform-specific scope validation
    if *platform == PlatformKind::GitHub {
        check_github_scopes(&mut status);
    }

    Ok(status)
}

/// Check GitHub token scopes for required and dangerous permissions.
fn check_github_scopes(status: &mut AuthStatus) {
    if status.scopes.is_empty() {
        // Fine-grained tokens don't return scopes via X-OAuth-Scopes
        // This is normal, not a warning
        return;
    }

    let required = ["repo"];
    let recommended = ["read:org"];
    let dangerous = ["delete_repo"];

    for scope in &required {
        if !status.scopes.iter().any(|s| s == scope) {
            status.warnings.push(format!(
                "Missing required scope: `{scope}`. Token may not be able to access private repos."
            ));
        }
    }

    for scope in &recommended {
        if !status.scopes.iter().any(|s| s == scope) {
            status.warnings.push(format!(
                "Missing recommended scope: `{scope}`. Organization discovery may be limited."
            ));
        }
    }

    for scope in &dangerous {
        if status.scopes.iter().any(|s| s == scope) {
            status.warnings.push(format!(
                "Dangerous scope detected: `{scope}`. Consider creating a token without this scope."
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_github_scopes_all_good() {
        let mut status = AuthStatus {
            valid: true,
            identity: "@testuser".to_string(),
            scopes: vec!["repo".to_string(), "read:org".to_string()],
            expires_at: None,
            warnings: vec![],
        };
        check_github_scopes(&mut status);
        assert!(status.warnings.is_empty());
    }

    #[test]
    fn check_github_scopes_missing_repo() {
        let mut status = AuthStatus {
            valid: true,
            identity: "@testuser".to_string(),
            scopes: vec!["read:org".to_string()],
            expires_at: None,
            warnings: vec![],
        };
        check_github_scopes(&mut status);
        assert_eq!(status.warnings.len(), 1);
        assert!(status.warnings[0].contains("repo"));
    }

    #[test]
    fn check_github_scopes_missing_read_org() {
        let mut status = AuthStatus {
            valid: true,
            identity: "@testuser".to_string(),
            scopes: vec!["repo".to_string()],
            expires_at: None,
            warnings: vec![],
        };
        check_github_scopes(&mut status);
        assert_eq!(status.warnings.len(), 1);
        assert!(status.warnings[0].contains("read:org"));
    }

    #[test]
    fn check_github_scopes_dangerous_delete_repo() {
        let mut status = AuthStatus {
            valid: true,
            identity: "@testuser".to_string(),
            scopes: vec![
                "repo".to_string(),
                "read:org".to_string(),
                "delete_repo".to_string(),
            ],
            expires_at: None,
            warnings: vec![],
        };
        check_github_scopes(&mut status);
        assert_eq!(status.warnings.len(), 1);
        assert!(status.warnings[0].contains("delete_repo"));
        assert!(status.warnings[0].contains("Dangerous"));
    }

    #[test]
    fn check_github_scopes_empty_is_ok() {
        // Fine-grained tokens don't have scopes
        let mut status = AuthStatus {
            valid: true,
            identity: "@testuser".to_string(),
            scopes: vec![],
            expires_at: None,
            warnings: vec![],
        };
        check_github_scopes(&mut status);
        assert!(status.warnings.is_empty());
    }

    #[test]
    fn check_github_scopes_multiple_warnings() {
        let mut status = AuthStatus {
            valid: true,
            identity: "@testuser".to_string(),
            scopes: vec!["delete_repo".to_string()], // has dangerous, missing required + recommended
            expires_at: None,
            warnings: vec![],
        };
        check_github_scopes(&mut status);
        assert_eq!(status.warnings.len(), 3); // missing repo, missing read:org, dangerous delete_repo
    }
}
