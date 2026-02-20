use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PulsosConfig {
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub github: GitHubConfig,
    #[serde(default)]
    pub railway: RailwayConfig,
    #[serde(default)]
    pub vercel: VercelConfig,
    #[serde(default)]
    pub correlations: Vec<CorrelationConfig>,
    #[serde(default)]
    pub views: Vec<ViewConfig>,
    #[serde(default)]
    pub groups: Vec<GroupConfig>,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub cache: CacheConfig,
}

// ── Authentication ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthConfig {
    #[serde(default = "default_github_host")]
    pub github_host: String,
    #[serde(default)]
    pub token_detection: TokenDetectionConfig,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            github_host: default_github_host(),
            token_detection: TokenDetectionConfig::default(),
        }
    }
}

fn default_github_host() -> String {
    "github.com".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenDetectionConfig {
    #[serde(default = "default_true")]
    pub detect_gh_cli: bool,
    #[serde(default = "default_true")]
    pub detect_railway_cli: bool,
    #[serde(default = "default_true")]
    pub detect_vercel_cli: bool,
    #[serde(default = "default_true")]
    pub detect_env_vars: bool,
}

impl Default for TokenDetectionConfig {
    fn default() -> Self {
        Self {
            detect_gh_cli: true,
            detect_railway_cli: true,
            detect_vercel_cli: true,
            detect_env_vars: true,
        }
    }
}

fn default_true() -> bool {
    true
}

// ── GitHub ──

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GitHubConfig {
    #[serde(default)]
    pub organizations: Vec<OrgConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrgConfig {
    pub name: String,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub auto_discover: bool,
}

// ── Railway ──

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RailwayConfig {
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceConfig {
    pub name: String,
    pub id: Option<String>,
    #[serde(default)]
    pub include_projects: Vec<String>,
    #[serde(default)]
    pub exclude_projects: Vec<String>,
    #[serde(default = "default_production")]
    pub default_environment: String,
}

fn default_production() -> String {
    "production".to_string()
}

// ── Vercel ──

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct VercelConfig {
    #[serde(default)]
    pub teams: Vec<TeamConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeamConfig {
    pub name: String,
    pub id: Option<String>,
    #[serde(default)]
    pub include_projects: Vec<String>,
    #[serde(default = "default_true")]
    pub include_preview_deployments: bool,
}

// ── Correlations ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorrelationConfig {
    pub name: String,
    pub github_repo: Option<String>,
    pub railway_project: Option<String>,
    pub railway_workspace: Option<String>,
    pub railway_environment: Option<String>,
    pub vercel_project: Option<String>,
    pub vercel_team: Option<String>,
    #[serde(default)]
    pub branch_mapping: HashMap<String, String>,
}

// ── Groups ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupConfig {
    pub name: String,
    #[serde(default)]
    pub resources: Vec<String>,
}

// ── Views ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ViewConfig {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub branch_filter: Option<String>,
    #[serde(default)]
    pub status_filter: Vec<String>,
    #[serde(default = "default_refresh")]
    pub refresh_interval: u64,
    #[serde(default)]
    pub vercel_include_previews: bool,
}

fn default_refresh() -> u64 {
    5
}

// ── TUI ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TuiConfig {
    #[serde(default = "default_refresh")]
    pub refresh_interval: u64,
    #[serde(default = "default_fps")]
    pub fps: u64,
    #[serde(default = "default_dark")]
    pub theme: String,
    #[serde(default = "default_auto")]
    pub unicode: String,
    #[serde(default = "default_unified")]
    pub default_tab: String,
    #[serde(default = "default_true")]
    pub show_sparklines: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            refresh_interval: 5,
            fps: 10,
            theme: "dark".to_string(),
            unicode: "auto".to_string(),
            default_tab: "unified".to_string(),
            show_sparklines: true,
        }
    }
}

fn default_fps() -> u64 {
    10
}
fn default_dark() -> String {
    "dark".to_string()
}
fn default_auto() -> String {
    "auto".to_string()
}
fn default_unified() -> String {
    "unified".to_string()
}

// ── Cache ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheConfig {
    pub directory: Option<String>,
    #[serde(default = "default_cache_mb")]
    pub max_size_mb: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            directory: None,
            max_size_mb: 100,
        }
    }
}

fn default_cache_mb() -> u64 {
    100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_roundtrip() {
        let config = PulsosConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: PulsosConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn parse_minimal_toml() {
        let toml_str = "";
        let config: PulsosConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.auth.github_host, "github.com");
        assert!(config.auth.token_detection.detect_gh_cli);
        assert!(config.github.organizations.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
[auth]
github_host = "github.mycompany.com"

[auth.token_detection]
detect_gh_cli = true
detect_railway_cli = false

[[github.organizations]]
name = "myorg"
include_patterns = ["api-*"]
exclude_patterns = ["*-legacy"]
auto_discover = true

[[railway.workspaces]]
name = "lambda-prod"
include_projects = ["my-saas-api"]
default_environment = "production"

[[vercel.teams]]
name = "lambda"
include_projects = ["my-saas-web"]
include_preview_deployments = true

[[correlations]]
name = "my-saas"
github_repo = "myorg/my-saas"
railway_project = "my-saas-api"
vercel_project = "my-saas-web"

[correlations.branch_mapping]
main = "production"
develop = "staging"

[[views]]
name = "production"
description = "Production systems"
projects = ["my-saas", "api-core"]
platforms = ["github", "railway", "vercel"]
branch_filter = "main"
status_filter = ["success", "failure"]
refresh_interval = 5

[tui]
theme = "light"
fps = 15

[cache]
max_size_mb = 50
"#;
        let config: PulsosConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.auth.github_host, "github.mycompany.com");
        assert!(!config.auth.token_detection.detect_railway_cli);
        assert_eq!(config.github.organizations.len(), 1);
        assert_eq!(config.github.organizations[0].name, "myorg");
        assert_eq!(config.railway.workspaces.len(), 1);
        assert_eq!(config.vercel.teams.len(), 1);
        assert_eq!(config.correlations.len(), 1);
        assert_eq!(
            config.correlations[0].branch_mapping.get("main"),
            Some(&"production".to_string())
        );
        assert_eq!(config.views.len(), 1);
        assert_eq!(config.tui.theme, "light");
        assert_eq!(config.tui.fps, 15);
        assert_eq!(config.cache.max_size_mb, 50);
    }

    #[test]
    fn defaults_are_correct() {
        let config = PulsosConfig::default();
        assert_eq!(config.auth.github_host, "github.com");
        assert!(config.auth.token_detection.detect_gh_cli);
        assert!(config.auth.token_detection.detect_env_vars);
        assert_eq!(config.tui.refresh_interval, 5);
        assert_eq!(config.tui.fps, 10);
        assert_eq!(config.tui.theme, "dark");
        assert!(config.tui.show_sparklines);
        assert_eq!(config.cache.max_size_mb, 100);
    }
}
