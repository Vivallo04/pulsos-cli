use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// The universal deployment status across all platforms.
/// Each platform's native status maps into one of these.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeploymentStatus {
    /// Waiting to start (GH: queued/waiting, RW: QUEUED/WAITING, VC: QUEUED)
    Queued,
    /// Actively building/running (GH: in_progress, RW: BUILDING/DEPLOYING/INITIALIZING, VC: BUILDING)
    InProgress,
    /// Completed successfully (GH: completed+success, RW: SUCCESS, VC: READY)
    Success,
    /// Completed with failure (GH: completed+failure, RW: FAILED/CRASHED, VC: ERROR)
    Failed,
    /// Cancelled by user (GH: completed+cancelled, RW: REMOVED, VC: CANCELED)
    Cancelled,
    /// Skipped (GH: completed+skipped, RW: SKIPPED)
    Skipped,
    /// Needs manual action (GH: completed+action_required, RW: NEEDS_APPROVAL)
    ActionRequired,
    /// Dormant/sleeping (RW: SLEEPING only)
    Sleeping,
    /// Unknown — platform returned a value we don't recognize
    Unknown(String),
}

impl Default for DeploymentStatus {
    fn default() -> Self {
        Self::Unknown("unknown".into())
    }
}

impl fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Success => write!(f, "success"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Skipped => write!(f, "skipped"),
            Self::ActionRequired => write!(f, "action_required"),
            Self::Sleeping => write!(f, "sleeping"),
            Self::Unknown(s) => write!(f, "unknown({s})"),
        }
    }
}

/// Source platform for a deployment event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    GitHub,
    Railway,
    Vercel,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub => write!(f, "GitHub"),
            Self::Railway => write!(f, "Railway"),
            Self::Vercel => write!(f, "Vercel"),
        }
    }
}

/// A single deployment event, normalized across platforms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentEvent {
    /// Platform-specific unique ID
    pub id: String,
    /// Which platform this came from
    pub platform: Platform,
    /// Normalized status
    pub status: DeploymentStatus,
    /// Git commit SHA (exact for GitHub/Vercel, absent for Railway)
    pub commit_sha: Option<String>,
    /// Git branch
    pub branch: Option<String>,
    /// Commit message or workflow display title
    pub title: Option<String>,
    /// Who triggered this deployment
    pub actor: Option<String>,
    /// When this event was created on the platform
    pub created_at: DateTime<Utc>,
    /// When this event was last updated
    pub updated_at: Option<DateTime<Utc>>,
    /// Duration in seconds (completed_at - started_at)
    pub duration_secs: Option<u64>,
    /// Platform-specific URL for viewing in browser
    pub url: Option<String>,
    /// Platform-specific extra data
    pub metadata: EventMetadata,
    /// True when this event was served from a local cache fallback rather than
    /// a live API response. Used to show a staleness indicator in the output.
    #[serde(default)]
    pub is_from_cache: bool,
}

/// Summary of a single workflow job (GitHub Actions pipeline stage).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobSummary {
    pub name: String,
    pub status: DeploymentStatus,
}

/// Summary of a single GitHub Actions step inside a workflow job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobStepSummary {
    pub number: u32,
    pub name: String,
    pub status: DeploymentStatus,
    pub duration_secs: Option<u64>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

/// Job-level details for GitHub Actions, including step breakdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobDetail {
    #[serde(default)]
    pub job_id: Option<u64>,
    pub name: String,
    pub status: DeploymentStatus,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub steps: Vec<JobStepSummary>,
}

/// Platform-specific metadata that doesn't fit the unified model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventMetadata {
    /// GitHub: workflow name ("CI", "Deploy")
    pub workflow_name: Option<String>,
    /// GitHub: trigger event ("push", "pull_request")
    pub trigger_event: Option<String>,
    /// Railway: service name
    pub service_name: Option<String>,
    /// Railway: environment name
    pub environment_name: Option<String>,
    /// Vercel: deployment URL ("my-app-abc123.vercel.app")
    pub preview_url: Option<String>,
    /// Vercel: target ("production" or preview)
    pub deploy_target: Option<String>,
    /// The tracked resource platform_id this event was fetched for.
    /// GitHub: "myorg/my-saas", Railway: "proj:svc:env", Vercel: "prj-001"
    #[serde(default)]
    pub source_id: Option<String>,
    /// GitHub: workflow job summaries (pipeline stages).
    #[serde(default)]
    pub jobs: Vec<JobSummary>,
    /// GitHub: per-job step details from the Actions jobs endpoint.
    #[serde(default)]
    pub job_details: Vec<JobDetail>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_status_display() {
        assert_eq!(DeploymentStatus::Success.to_string(), "success");
        assert_eq!(DeploymentStatus::Failed.to_string(), "failed");
        assert_eq!(DeploymentStatus::InProgress.to_string(), "in_progress");
        assert_eq!(
            DeploymentStatus::Unknown("foo".into()).to_string(),
            "unknown(foo)"
        );
    }

    #[test]
    fn platform_display() {
        assert_eq!(Platform::GitHub.to_string(), "GitHub");
        assert_eq!(Platform::Railway.to_string(), "Railway");
        assert_eq!(Platform::Vercel.to_string(), "Vercel");
    }

    #[test]
    fn deployment_status_equality() {
        assert_eq!(DeploymentStatus::Success, DeploymentStatus::Success);
        assert_ne!(DeploymentStatus::Success, DeploymentStatus::Failed);
    }

    #[test]
    fn deployment_status_serialization_roundtrip() {
        let status = DeploymentStatus::InProgress;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: DeploymentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }
}
