use crate::domain::deployment::DeploymentStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// GET /repos/{owner}/{repo}/actions/runs
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkflowRunsResponse {
    pub total_count: u64,
    pub workflow_runs: Vec<WorkflowRun>,
}

/// A single workflow run from the GitHub Actions API.
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: Option<String>,
    pub head_branch: Option<String>,
    pub head_sha: String,
    pub path: Option<String>,
    pub run_number: u64,
    pub event: String,
    pub display_title: Option<String>,
    pub status: GhRunStatus,
    pub conclusion: Option<GhConclusion>,
    pub workflow_id: u64,
    pub html_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub run_started_at: Option<DateTime<Utc>>,
    pub actor: Option<GhActor>,
}

impl WorkflowRun {
    pub fn to_deployment_status(&self) -> DeploymentStatus {
        match self.status {
            GhRunStatus::Queued
            | GhRunStatus::Waiting
            | GhRunStatus::Requested
            | GhRunStatus::Pending => DeploymentStatus::Queued,
            GhRunStatus::InProgress => DeploymentStatus::InProgress,
            GhRunStatus::Completed => match self.conclusion {
                Some(GhConclusion::Success)
                | Some(GhConclusion::Neutral)
                | Some(GhConclusion::Stale) => DeploymentStatus::Success,
                Some(GhConclusion::Failure)
                | Some(GhConclusion::StartupFailure)
                | Some(GhConclusion::TimedOut) => DeploymentStatus::Failed,
                Some(GhConclusion::Cancelled) => DeploymentStatus::Cancelled,
                Some(GhConclusion::Skipped) => DeploymentStatus::Skipped,
                Some(GhConclusion::ActionRequired) => DeploymentStatus::ActionRequired,
                Some(GhConclusion::Unknown) => {
                    DeploymentStatus::Unknown("unknown_conclusion".into())
                }
                None => DeploymentStatus::Unknown("completed_no_conclusion".to_string()),
            },
            GhRunStatus::Unknown => DeploymentStatus::Unknown("unknown_status".into()),
        }
    }
}

/// GitHub workflow run status values.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GhRunStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
    #[serde(other)]
    Unknown,
}

/// GitHub workflow run conclusion values.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GhConclusion {
    Success,
    Failure,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
    Neutral,
    Stale,
    StartupFailure,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GhActor {
    pub login: String,
    pub id: u64,
    pub avatar_url: Option<String>,
}

/// GET /repos/{owner}/{repo}/actions/runs/{id}/jobs
#[derive(Debug, Deserialize)]
pub struct WorkflowJobsResponse {
    pub total_count: u64,
    pub jobs: Vec<WorkflowJob>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowJob {
    pub id: u64,
    pub run_id: u64,
    pub head_sha: String,
    pub status: GhRunStatus,
    pub conclusion: Option<GhConclusion>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub name: String,
    pub steps: Option<Vec<WorkflowStep>>,
    pub html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowStep {
    pub name: String,
    pub status: GhRunStatus,
    pub conclusion: Option<GhConclusion>,
    pub number: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// GET /user/orgs
#[derive(Debug, Deserialize)]
pub struct GhOrg {
    pub login: String,
    pub id: u64,
    pub description: Option<String>,
}

/// GET /user/repos or GET /orgs/{org}/repos
#[derive(Debug, Deserialize)]
pub struct GhRepo {
    pub id: u64,
    pub full_name: String,
    pub name: String,
    pub private: bool,
    pub archived: bool,
    pub disabled: bool,
    pub default_branch: Option<String>,
    pub html_url: String,
    pub permissions: Option<GhRepoPermissions>,
    pub owner: GhOwner,
}

#[derive(Debug, Deserialize)]
pub struct GhRepoPermissions {
    pub admin: bool,
    pub push: bool,
    pub pull: bool,
}

#[derive(Debug, Deserialize)]
pub struct GhOwner {
    pub login: String,
    pub id: u64,
    #[serde(rename = "type")]
    pub owner_type: String,
}

/// Rate limit tracking from response headers (not a JSON response).
#[derive(Debug, Clone)]
pub struct GhRateLimit {
    pub limit: u32,
    pub remaining: u32,
    pub reset: DateTime<Utc>,
    pub used: u32,
}

/// GET /user response for auth validation
#[derive(Debug, Deserialize)]
pub struct GhUser {
    pub login: String,
    pub id: u64,
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_queued() {
        let run = make_run(GhRunStatus::Queued, None);
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Queued);
    }

    #[test]
    fn status_mapping_in_progress() {
        let run = make_run(GhRunStatus::InProgress, None);
        assert_eq!(run.to_deployment_status(), DeploymentStatus::InProgress);
    }

    #[test]
    fn status_mapping_success() {
        let run = make_run(GhRunStatus::Completed, Some(GhConclusion::Success));
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Success);
    }

    #[test]
    fn status_mapping_failure() {
        let run = make_run(GhRunStatus::Completed, Some(GhConclusion::Failure));
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Failed);
    }

    #[test]
    fn status_mapping_timed_out() {
        let run = make_run(GhRunStatus::Completed, Some(GhConclusion::TimedOut));
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Failed);
    }

    #[test]
    fn status_mapping_cancelled() {
        let run = make_run(GhRunStatus::Completed, Some(GhConclusion::Cancelled));
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Cancelled);
    }

    #[test]
    fn status_mapping_skipped() {
        let run = make_run(GhRunStatus::Completed, Some(GhConclusion::Skipped));
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Skipped);
    }

    #[test]
    fn status_mapping_action_required() {
        let run = make_run(GhRunStatus::Completed, Some(GhConclusion::ActionRequired));
        assert_eq!(run.to_deployment_status(), DeploymentStatus::ActionRequired);
    }

    #[test]
    fn status_mapping_neutral_is_success() {
        let run = make_run(GhRunStatus::Completed, Some(GhConclusion::Neutral));
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Success);
    }

    #[test]
    fn status_mapping_completed_no_conclusion() {
        let run = make_run(GhRunStatus::Completed, None);
        assert!(matches!(
            run.to_deployment_status(),
            DeploymentStatus::Unknown(_)
        ));
    }

    #[test]
    fn status_mapping_waiting() {
        let run = make_run(GhRunStatus::Waiting, None);
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Queued);
    }

    #[test]
    fn deserialize_workflow_run() {
        let json = r#"{
            "id": 12345,
            "name": "CI",
            "head_branch": "main",
            "head_sha": "abc123def456",
            "run_number": 42,
            "event": "push",
            "display_title": "Fix login bug",
            "status": "completed",
            "conclusion": "success",
            "workflow_id": 99,
            "html_url": "https://github.com/org/repo/actions/runs/12345",
            "created_at": "2026-02-15T10:00:00Z",
            "updated_at": "2026-02-15T10:05:00Z",
            "run_started_at": "2026-02-15T10:00:30Z"
        }"#;
        let run: WorkflowRun = serde_json::from_str(json).unwrap();
        assert_eq!(run.id, 12345);
        assert_eq!(run.head_sha, "abc123def456");
        assert_eq!(run.status, GhRunStatus::Completed);
        assert_eq!(run.conclusion, Some(GhConclusion::Success));
        assert_eq!(run.to_deployment_status(), DeploymentStatus::Success);
    }

    fn make_run(status: GhRunStatus, conclusion: Option<GhConclusion>) -> WorkflowRun {
        WorkflowRun {
            id: 1,
            name: Some("CI".into()),
            head_branch: Some("main".into()),
            head_sha: "abc123".into(),
            path: None,
            run_number: 1,
            event: "push".into(),
            display_title: None,
            status,
            conclusion,
            workflow_id: 1,
            html_url: "https://github.com".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            run_started_at: None,
            actor: None,
        }
    }
}
