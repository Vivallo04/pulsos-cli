use super::deployment::{DeploymentEvent, Platform};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A unified project — the core abstraction that maps across platforms.
/// Created during first-run wizard and stored in config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedProject {
    /// User-facing name (e.g., "my-saas")
    pub name: String,
    /// GitHub binding (if connected)
    pub github: Option<GitHubBinding>,
    /// Railway binding (if connected)
    pub railway: Option<RailwayBinding>,
    /// Vercel binding (if connected)
    pub vercel: Option<VercelBinding>,
    /// Most recent events per platform (populated at runtime)
    pub events: Vec<DeploymentEvent>,
    /// Computed health score (0–100, populated at runtime)
    pub health_score: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubBinding {
    /// "myorg/my-saas"
    pub repo_full_name: String,
    /// ["ci.yml", "deploy.yml"] or empty for all
    pub workflows: Vec<String>,
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailwayBinding {
    pub project_id: String,
    pub project_name: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub services: Vec<RailwayServiceRef>,
    pub environment_id: String,
    /// "production", "staging"
    pub environment_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailwayServiceRef {
    pub service_id: String,
    pub service_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VercelBinding {
    pub project_id: String,
    pub project_name: String,
    pub team_id: Option<String>,
    pub team_name: Option<String>,
    /// "myorg/my-saas" (from project.link.repo)
    pub linked_repo: Option<String>,
    pub include_previews: bool,
}

/// Correlation confidence level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Confidence {
    /// No correlation found
    Unmatched,
    /// Timestamp-only heuristic
    Low,
    /// Explicit config mapping + timestamp match
    High,
    /// SHA match — GitHub ↔ Vercel only
    Exact,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => write!(f, "● Exact"),
            Self::High => write!(f, "◐ High"),
            Self::Low => write!(f, "○ Low"),
            Self::Unmatched => write!(f, "? Unmatched"),
        }
    }
}

/// A correlated deployment event — links events across platforms
/// for the same commit/deployment.
#[derive(Debug, Clone, Serialize)]
pub struct CorrelatedEvent {
    pub commit_sha: Option<String>,
    pub github: Option<DeploymentEvent>,
    pub railway: Option<DeploymentEvent>,
    pub vercel: Option<DeploymentEvent>,
    pub confidence: Confidence,
    pub timestamp: DateTime<Utc>,
    /// True if any constituent platform event was served from a local cache
    /// fallback rather than a live API response.
    pub is_stale: bool,
}

impl UnifiedProject {
    /// Get the latest event for a given platform.
    pub fn latest_event(&self, platform: Platform) -> Option<&DeploymentEvent> {
        self.events
            .iter()
            .filter(|e| e.platform == platform)
            .max_by_key(|e| e.created_at)
    }

    /// Get connected platforms for this project.
    pub fn connected_platforms(&self) -> Vec<Platform> {
        let mut platforms = Vec::new();
        if self.github.is_some() {
            platforms.push(Platform::GitHub);
        }
        if self.railway.is_some() {
            platforms.push(Platform::Railway);
        }
        if self.vercel.is_some() {
            platforms.push(Platform::Vercel);
        }
        platforms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_ordering() {
        assert!(Confidence::Exact > Confidence::High);
        assert!(Confidence::High > Confidence::Low);
        assert!(Confidence::Low > Confidence::Unmatched);
    }

    #[test]
    fn confidence_display() {
        assert_eq!(Confidence::Exact.to_string(), "● Exact");
        assert_eq!(Confidence::High.to_string(), "◐ High");
        assert_eq!(Confidence::Low.to_string(), "○ Low");
        assert_eq!(Confidence::Unmatched.to_string(), "? Unmatched");
    }

    #[test]
    fn unified_project_connected_platforms() {
        let project = UnifiedProject {
            name: "test".into(),
            github: Some(GitHubBinding {
                repo_full_name: "org/repo".into(),
                workflows: vec![],
                default_branch: None,
            }),
            railway: None,
            vercel: Some(VercelBinding {
                project_id: "prj_1".into(),
                project_name: "test-web".into(),
                team_id: None,
                team_name: None,
                linked_repo: None,
                include_previews: false,
            }),
            events: vec![],
            health_score: None,
        };
        let platforms = project.connected_platforms();
        assert_eq!(platforms, vec![Platform::GitHub, Platform::Vercel]);
    }
}
