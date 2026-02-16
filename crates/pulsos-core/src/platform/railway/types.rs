use crate::domain::deployment::DeploymentStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Railway uses Relay-style pagination: edges → node.
#[derive(Debug, Deserialize)]
pub struct Connection<T> {
    pub edges: Vec<Edge<T>>,
    #[serde(default)]
    pub page_info: Option<PageInfo>,
}

#[derive(Debug, Deserialize)]
pub struct Edge<T> {
    pub node: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

/// Root response wrapper for GraphQL.
#[derive(Debug, Deserialize)]
pub struct GqlResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Deserialize)]
pub struct GqlError {
    pub message: String,
    pub extensions: Option<serde_json::Value>,
}

// ── Query response shapes ──

#[derive(Debug, Deserialize)]
pub struct ProjectsData {
    pub projects: Connection<RwProject>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RwProject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub services: Connection<RwService>,
    pub environments: Connection<RwEnvironment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RwService {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RwEnvironment {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInstanceData {
    pub service_instance: RwServiceInstance,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RwServiceInstance {
    pub id: String,
    pub service_name: String,
    pub start_command: Option<String>,
    pub build_command: Option<String>,
    pub root_directory: Option<String>,
    pub healthcheck_path: Option<String>,
    pub region: Option<String>,
    pub num_replicas: Option<u32>,
    pub restart_policy_type: Option<String>,
    pub restart_policy_max_retries: Option<u32>,
    pub latest_deployment: Option<RwDeployment>,
}

#[derive(Debug, Deserialize)]
pub struct DeploymentsData {
    pub deployments: Connection<RwDeployment>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RwDeployment {
    pub id: String,
    pub status: RwDeploymentStatus,
    pub created_at: DateTime<Utc>,
    pub static_url: Option<String>,
}

/// Railway deployment status values.
///
/// These represent the *deployment* outcome, NOT the service's current
/// runtime health. Railway does not continuously monitor service health
/// after deployment completes.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RwDeploymentStatus {
    Building,
    Crashed,
    Deploying,
    Failed,
    Initializing,
    NeedsApproval,
    Queued,
    Removed,
    Removing,
    Skipped,
    Sleeping,
    Success,
    Waiting,
}

impl From<RwDeploymentStatus> for DeploymentStatus {
    fn from(s: RwDeploymentStatus) -> Self {
        match s {
            RwDeploymentStatus::Queued | RwDeploymentStatus::Waiting => DeploymentStatus::Queued,
            RwDeploymentStatus::Building
            | RwDeploymentStatus::Deploying
            | RwDeploymentStatus::Initializing => DeploymentStatus::InProgress,
            RwDeploymentStatus::Success => DeploymentStatus::Success,
            RwDeploymentStatus::Failed | RwDeploymentStatus::Crashed => DeploymentStatus::Failed,
            RwDeploymentStatus::Removed | RwDeploymentStatus::Removing => {
                DeploymentStatus::Cancelled
            }
            RwDeploymentStatus::Skipped => DeploymentStatus::Skipped,
            RwDeploymentStatus::NeedsApproval => DeploymentStatus::ActionRequired,
            RwDeploymentStatus::Sleeping => DeploymentStatus::Sleeping,
        }
    }
}

/// Workspaces from the me/teams query.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RwTeam {
    pub id: String,
    pub name: String,
}

/// Response for me query (auth validation).
#[derive(Debug, Deserialize)]
pub struct MeData {
    pub me: RwMe,
}

#[derive(Debug, Deserialize)]
pub struct RwMe {
    pub id: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

/// Response for teams query.
#[derive(Debug, Deserialize)]
pub struct TeamsData {
    pub teams: Connection<RwTeam>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn railway_status_mapping() {
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Queued),
            DeploymentStatus::Queued
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Waiting),
            DeploymentStatus::Queued
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Building),
            DeploymentStatus::InProgress
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Deploying),
            DeploymentStatus::InProgress
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Initializing),
            DeploymentStatus::InProgress
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Success),
            DeploymentStatus::Success
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Failed),
            DeploymentStatus::Failed
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Crashed),
            DeploymentStatus::Failed
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Removed),
            DeploymentStatus::Cancelled
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Skipped),
            DeploymentStatus::Skipped
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::NeedsApproval),
            DeploymentStatus::ActionRequired
        );
        assert_eq!(
            DeploymentStatus::from(RwDeploymentStatus::Sleeping),
            DeploymentStatus::Sleeping
        );
    }

    #[test]
    fn deserialize_deployment() {
        let json = r#"{
            "id": "deploy-123",
            "status": "SUCCESS",
            "createdAt": "2026-02-15T10:00:00Z",
            "staticUrl": "my-app.up.railway.app"
        }"#;
        let deployment: RwDeployment = serde_json::from_str(json).unwrap();
        assert_eq!(deployment.id, "deploy-123");
        assert_eq!(deployment.status, RwDeploymentStatus::Success);
        assert_eq!(
            DeploymentStatus::from(deployment.status),
            DeploymentStatus::Success
        );
    }

    #[test]
    fn deserialize_graphql_response() {
        let json = r#"{
            "data": {
                "deployments": {
                    "edges": [
                        {
                            "node": {
                                "id": "d1",
                                "status": "BUILDING",
                                "createdAt": "2026-02-15T10:00:00Z"
                            }
                        }
                    ]
                }
            }
        }"#;
        let resp: GqlResponse<DeploymentsData> = serde_json::from_str(json).unwrap();
        let data = resp.data.unwrap();
        assert_eq!(data.deployments.edges.len(), 1);
        assert_eq!(
            data.deployments.edges[0].node.status,
            RwDeploymentStatus::Building
        );
    }
}
