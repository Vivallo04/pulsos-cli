//! Detects tokens from existing CLI tool configuration files.
//!
//! Supports:
//! - GitHub CLI (`gh`): `~/.config/gh/hosts.yml`
//! - Railway CLI: `~/.railway/config.json`
//! - Vercel CLI: `~/.config/com.vercel.cli/auth.json` or `~/.vercel/auth.json`

use std::path::{Path, PathBuf};

/// Detect a GitHub token from the `gh` CLI config.
///
/// Reads `~/.config/gh/hosts.yml` (or `$GH_CONFIG_DIR/hosts.yml`) and
/// extracts the `oauth_token` for `github.com`.
pub fn detect_gh_token() -> Option<String> {
    detect_gh_token_from_path(&gh_config_path()?)
}

/// Detect a Railway token from the Railway CLI config.
///
/// Reads `~/.railway/config.json` and extracts the token.
pub fn detect_railway_token() -> Option<String> {
    detect_railway_token_from_path(&railway_config_path()?)
}

/// Detect a Vercel token from the Vercel CLI config.
///
/// Checks `~/.config/com.vercel.cli/auth.json` first, then falls back
/// to `~/.vercel/auth.json` (older versions).
pub fn detect_vercel_token() -> Option<String> {
    for path in vercel_config_paths() {
        if let Some(token) = detect_vercel_token_from_path(&path) {
            return Some(token);
        }
    }
    None
}

// ── Path resolution ──

fn gh_config_path() -> Option<PathBuf> {
    // Respect $GH_CONFIG_DIR if set
    if let Ok(dir) = std::env::var("GH_CONFIG_DIR") {
        let path = PathBuf::from(dir).join("hosts.yml");
        if path.exists() {
            return Some(path);
        }
    }

    // Default: ~/.config/gh/hosts.yml
    let config_dir = dirs::config_dir()?;
    let path = config_dir.join("gh").join("hosts.yml");
    if path.exists() {
        Some(path)
    } else {
        tracing::debug!("gh CLI config not found at {}", path.display());
        None
    }
}

fn railway_config_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let path = home.join(".railway").join("config.json");
    if path.exists() {
        Some(path)
    } else {
        tracing::debug!("Railway CLI config not found at {}", path.display());
        None
    }
}

fn vercel_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // New location: ~/.config/com.vercel.cli/auth.json
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("com.vercel.cli").join("auth.json"));
    }

    // Old location: ~/.vercel/auth.json
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".vercel").join("auth.json"));
    }

    paths
}

// ── Parsing (testable with path injection) ──

fn detect_gh_token_from_path(path: &Path) -> Option<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("Failed to read gh config at {}: {}", path.display(), e);
            return None;
        }
    };

    parse_gh_hosts_yml(&content)
}

/// Parse the gh CLI hosts.yml format.
///
/// The format is roughly:
/// ```yaml
/// github.com:
///     oauth_token: gho_xxxxxxxxxxxx
///     user: username
///     git_protocol: https
/// ```
///
/// We use a simple line-by-line parser to avoid a YAML dependency.
fn parse_gh_hosts_yml(content: &str) -> Option<String> {
    let mut in_github_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check for github.com host block
        if trimmed == "github.com:" || trimmed == "\"github.com\":" {
            in_github_block = true;
            continue;
        }

        // If we hit another top-level key (no leading whitespace), leave the block
        if in_github_block
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.is_empty()
        {
            in_github_block = false;
        }

        if in_github_block {
            if let Some(token) = trimmed.strip_prefix("oauth_token:") {
                let token = token.trim().trim_matches('"').trim_matches('\'');
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
    }

    tracing::debug!("No oauth_token found in gh hosts.yml");
    None
}

fn detect_railway_token_from_path(path: &Path) -> Option<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("Failed to read Railway config at {}: {}", path.display(), e);
            return None;
        }
    };

    parse_railway_config(&content)
}

/// Parse Railway CLI config.json.
///
/// Expected format:
/// ```json
/// {
///   "token": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
/// }
/// ```
fn parse_railway_config(content: &str) -> Option<String> {
    let parsed: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!("Failed to parse Railway config JSON: {}", e);
            return None;
        }
    };

    parsed
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
}

fn detect_vercel_token_from_path(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("Failed to read Vercel config at {}: {}", path.display(), e);
            return None;
        }
    };

    parse_vercel_auth(&content)
}

/// Parse Vercel CLI auth.json.
///
/// Expected format:
/// ```json
/// {
///   "token": "xxxxxxxxxxxxxxxxxxxxxxxx"
/// }
/// ```
fn parse_vercel_auth(content: &str) -> Option<String> {
    let parsed: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!("Failed to parse Vercel auth JSON: {}", e);
            return None;
        }
    };

    parsed
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // ── gh CLI ──

    #[test]
    fn parse_gh_hosts_yml_standard_format() {
        let content = r#"github.com:
    oauth_token: gho_test123abc
    user: testuser
    git_protocol: https
"#;
        assert_eq!(
            parse_gh_hosts_yml(content),
            Some("gho_test123abc".to_string())
        );
    }

    #[test]
    fn parse_gh_hosts_yml_quoted_host() {
        let content = r#""github.com":
    oauth_token: gho_quoted456
    user: testuser
"#;
        assert_eq!(
            parse_gh_hosts_yml(content),
            Some("gho_quoted456".to_string())
        );
    }

    #[test]
    fn parse_gh_hosts_yml_quoted_token() {
        let content = r#"github.com:
    oauth_token: "gho_quotedtoken"
    user: testuser
"#;
        assert_eq!(
            parse_gh_hosts_yml(content),
            Some("gho_quotedtoken".to_string())
        );
    }

    #[test]
    fn parse_gh_hosts_yml_multiple_hosts() {
        let content = r#"github.mycompany.com:
    oauth_token: gho_enterprise
    user: enterprise_user
github.com:
    oauth_token: gho_personal
    user: personal_user
"#;
        assert_eq!(
            parse_gh_hosts_yml(content),
            Some("gho_personal".to_string())
        );
    }

    #[test]
    fn parse_gh_hosts_yml_no_github_com() {
        let content = r#"github.mycompany.com:
    oauth_token: gho_enterprise
    user: enterprise_user
"#;
        assert_eq!(parse_gh_hosts_yml(content), None);
    }

    #[test]
    fn parse_gh_hosts_yml_empty_token() {
        let content = r#"github.com:
    oauth_token:
    user: testuser
"#;
        assert_eq!(parse_gh_hosts_yml(content), None);
    }

    #[test]
    fn detect_gh_token_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts.yml");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            "github.com:\n    oauth_token: gho_fromfile\n    user: test"
        )
        .unwrap();

        assert_eq!(
            detect_gh_token_from_path(&path),
            Some("gho_fromfile".to_string())
        );
    }

    #[test]
    fn detect_gh_token_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent_gh_hosts.yml");
        assert_eq!(detect_gh_token_from_path(&path), None);
    }

    // ── Railway ──

    #[test]
    fn parse_railway_config_standard() {
        let content = r#"{"token": "rw-test-token-123"}"#;
        assert_eq!(
            parse_railway_config(content),
            Some("rw-test-token-123".to_string())
        );
    }

    #[test]
    fn parse_railway_config_empty_token() {
        let content = r#"{"token": ""}"#;
        assert_eq!(parse_railway_config(content), None);
    }

    #[test]
    fn parse_railway_config_no_token_key() {
        let content = r#"{"other": "value"}"#;
        assert_eq!(parse_railway_config(content), None);
    }

    #[test]
    fn parse_railway_config_invalid_json() {
        let content = "not json";
        assert_eq!(parse_railway_config(content), None);
    }

    #[test]
    fn detect_railway_token_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"token": "rw-file-token"}"#).unwrap();

        assert_eq!(
            detect_railway_token_from_path(&path),
            Some("rw-file-token".to_string())
        );
    }

    // ── Vercel ──

    #[test]
    fn parse_vercel_auth_standard() {
        let content = r#"{"token": "vc_test_token_456"}"#;
        assert_eq!(
            parse_vercel_auth(content),
            Some("vc_test_token_456".to_string())
        );
    }

    #[test]
    fn parse_vercel_auth_empty_token() {
        let content = r#"{"token": ""}"#;
        assert_eq!(parse_vercel_auth(content), None);
    }

    #[test]
    fn parse_vercel_auth_no_token_key() {
        let content = r#"{"email": "user@example.com"}"#;
        assert_eq!(parse_vercel_auth(content), None);
    }

    #[test]
    fn detect_vercel_token_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        std::fs::write(&path, r#"{"token": "vc_file_token"}"#).unwrap();

        assert_eq!(
            detect_vercel_token_from_path(&path),
            Some("vc_file_token".to_string())
        );
    }

    #[test]
    fn detect_vercel_token_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent_vercel_auth.json");
        assert_eq!(detect_vercel_token_from_path(&path), None);
    }
}
