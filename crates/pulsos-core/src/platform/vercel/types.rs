use crate::domain::deployment::DeploymentStatus;
use serde::{Deserialize, Serialize};

/// GET /v6/deployments (list response)
#[derive(Debug, Deserialize)]
pub struct DeploymentsResponse {
    #[serde(default)]
    pub deployments: Vec<VcDeployment>,
    pub pagination: Option<VcPagination>,
}

#[derive(Debug, Deserialize)]
pub struct VcPagination {
    pub count: Option<u64>,
    pub next: Option<serde_json::Value>,
    pub prev: Option<serde_json::Value>,
}

/// A single Vercel deployment.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcDeployment {
    /// "dpl_xxx" — unique deployment ID.
    /// `/v6/deployments` uses `uid`; `/v9/projects` `latestDeployments` uses `id`.
    #[serde(alias = "id", default)]
    pub uid: String,
    /// Project name
    pub name: String,
    /// "my-app-abc123.vercel.app"
    pub url: Option<String>,
    /// Creation time as Unix ms (`created`) or RFC3339/string (`createdAt`) depending on endpoint.
    #[serde(default, alias = "createdAt")]
    pub created: Option<VcTimestamp>,
    pub state: Option<VcState>,
    pub ready_state: Option<VcState>,
    #[serde(rename = "type")]
    pub deploy_type: Option<String>,
    pub creator: Option<VcCreator>,
    pub meta: Option<VcMeta>,
    /// "production" or null (preview)
    pub target: Option<String>,
    pub alias_assigned: Option<serde_json::Value>,
    pub building_at: Option<u64>,
    pub ready: Option<u64>,
}

/// Vercel timestamp fields can be Unix milliseconds or string timestamps.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum VcTimestamp {
    Millis(u64),
    Text(String),
}

/// Vercel deployment meta — this is where git information lives.
/// Auto-populated by the GitHub integration for git-connected projects.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcMeta {
    pub github_commit_sha: Option<String>,
    pub github_commit_ref: Option<String>,
    pub github_commit_message: Option<String>,
    pub github_commit_author_name: Option<String>,
    pub github_commit_org: Option<String>,
    pub github_commit_repo: Option<String>,
    pub github_deployment: Option<String>,
}

/// Vercel deployment state.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VcState {
    Queued,
    Building,
    Ready,
    Error,
    Canceled,
    #[serde(other)]
    Unknown,
}

impl From<VcState> for DeploymentStatus {
    fn from(s: VcState) -> Self {
        match s {
            VcState::Queued => DeploymentStatus::Queued,
            VcState::Building => DeploymentStatus::InProgress,
            VcState::Ready => DeploymentStatus::Success,
            VcState::Error => DeploymentStatus::Failed,
            VcState::Canceled => DeploymentStatus::Cancelled,
            VcState::Unknown => DeploymentStatus::Unknown("vercel_unknown_state".into()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VcCreator {
    pub uid: String,
    pub username: Option<String>,
    pub email: Option<String>,
}

/// GET /v9/projects (list response)
#[derive(Debug, Deserialize)]
pub struct ProjectsResponse {
    #[serde(default)]
    pub projects: Vec<VcProject>,
    pub pagination: Option<VcPagination>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcProject {
    pub id: String,
    pub name: String,
    pub framework: Option<String>,
    pub link: Option<VcProjectLink>,
    pub latest_deployments: Option<Vec<VcDeployment>>,
    pub account_id: Option<String>,
}

/// Project link — tells us which GitHub repo is connected.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcProjectLink {
    #[serde(rename = "type")]
    pub link_type: Option<String>,
    /// "myorg/my-saas"
    pub repo: Option<String>,
    pub repo_id: Option<u64>,
    pub org: Option<String>,
}

/// GET /v2/teams
#[derive(Debug, Deserialize)]
pub struct TeamsResponse {
    #[serde(default)]
    pub teams: Vec<VcTeam>,
    pub pagination: Option<VcPagination>,
}

#[derive(Debug, Deserialize)]
pub struct VcTeam {
    pub id: String,
    pub name: String,
    pub slug: Option<String>,
}

/// GET /v2/user response for auth validation
#[derive(Debug, Deserialize)]
pub struct VcUser {
    /// Vercel personal tokens return `"id"`, team tokens return `"uid"`.
    #[serde(alias = "id")]
    pub uid: String,
    pub username: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
}

/// Wrapper for /v2/user endpoint.
/// Vercel personal-token responses may return the user nested under a `user` key
/// or flat at the root level — the untagged enum handles both.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum VcUserResponse {
    Nested { user: VcUser },
    Flat(VcUser),
}

impl VcUserResponse {
    pub fn into_user(self) -> VcUser {
        match self {
            VcUserResponse::Nested { user } => user,
            VcUserResponse::Flat(u) => u,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vercel_status_mapping() {
        assert_eq!(
            DeploymentStatus::from(VcState::Queued),
            DeploymentStatus::Queued
        );
        assert_eq!(
            DeploymentStatus::from(VcState::Building),
            DeploymentStatus::InProgress
        );
        assert_eq!(
            DeploymentStatus::from(VcState::Ready),
            DeploymentStatus::Success
        );
        assert_eq!(
            DeploymentStatus::from(VcState::Error),
            DeploymentStatus::Failed
        );
        assert_eq!(
            DeploymentStatus::from(VcState::Canceled),
            DeploymentStatus::Cancelled
        );
    }

    #[test]
    fn deserialize_deployment() {
        let json = r#"{
            "uid": "dpl_abc123",
            "name": "my-app",
            "url": "my-app-abc123.vercel.app",
            "created": 1708000000000,
            "state": "READY",
            "target": "production",
            "meta": {
                "githubCommitSha": "abc123def456",
                "githubCommitRef": "main",
                "githubCommitMessage": "Fix login bug",
                "githubCommitAuthorName": "vivallo",
                "githubCommitOrg": "myorg",
                "githubCommitRepo": "my-saas"
            }
        }"#;
        let deployment: VcDeployment = serde_json::from_str(json).unwrap();
        assert_eq!(deployment.uid, "dpl_abc123");
        assert_eq!(deployment.created, Some(VcTimestamp::Millis(1708000000000)));
        assert_eq!(deployment.state, Some(VcState::Ready));
        assert_eq!(deployment.target, Some("production".to_string()));

        let meta = deployment.meta.unwrap();
        assert_eq!(meta.github_commit_sha, Some("abc123def456".to_string()));
        assert_eq!(meta.github_commit_ref, Some("main".to_string()));
    }

    #[test]
    fn deserialize_deployment_with_id_alias() {
        // /v9/projects latestDeployments uses "id" instead of "uid"
        let json = r#"{
            "id": "dpl_xyz789",
            "name": "my-app",
            "created": 1708000000000,
            "state": "READY"
        }"#;
        let deployment: VcDeployment = serde_json::from_str(json).unwrap();
        assert_eq!(deployment.uid, "dpl_xyz789");
        assert_eq!(deployment.created, Some(VcTimestamp::Millis(1708000000000)));
    }

    #[test]
    fn deserialize_deployment_missing_id_defaults_empty() {
        // Some embedded deployment objects omit the ID field entirely
        let json = r#"{
            "name": "my-app",
            "created": 1708000000000,
            "state": "BUILDING"
        }"#;
        let deployment: VcDeployment = serde_json::from_str(json).unwrap();
        assert_eq!(deployment.uid, "");
    }

    #[test]
    fn deserialize_deployment_with_created_at_alias() {
        let json = r#"{
            "id": "dpl_aliased",
            "name": "my-app",
            "createdAt": 1708000000000,
            "state": "READY"
        }"#;
        let deployment: VcDeployment = serde_json::from_str(json).unwrap();
        assert_eq!(deployment.created, Some(VcTimestamp::Millis(1708000000000)));
    }

    #[test]
    fn deserialize_deployment_with_created_at_text() {
        let json = r#"{
            "id": "dpl_text",
            "name": "my-app",
            "createdAt": "2026-02-20T10:00:00.000Z",
            "state": "READY"
        }"#;
        let deployment: VcDeployment = serde_json::from_str(json).unwrap();
        assert_eq!(
            deployment.created,
            Some(VcTimestamp::Text("2026-02-20T10:00:00.000Z".to_string()))
        );
    }

    #[test]
    fn deserialize_deployment_missing_created_is_ok() {
        let json = r#"{
            "id": "dpl_no_created",
            "name": "my-app",
            "state": "READY"
        }"#;
        let deployment: VcDeployment = serde_json::from_str(json).unwrap();
        assert_eq!(deployment.created, None);
    }

    #[test]
    fn deserialize_project_with_link() {
        let json = r#"{
            "id": "prj_123",
            "name": "my-saas-web",
            "framework": "nextjs",
            "link": {
                "type": "github",
                "repo": "myorg/my-saas",
                "repoId": 12345,
                "org": "myorg"
            }
        }"#;
        let project: VcProject = serde_json::from_str(json).unwrap();
        assert_eq!(project.name, "my-saas-web");
        let link = project.link.unwrap();
        assert_eq!(link.link_type, Some("github".to_string()));
        assert_eq!(link.repo, Some("myorg/my-saas".to_string()));
    }
}
