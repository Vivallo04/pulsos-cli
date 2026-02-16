use crate::cache::store::CacheStore;
use crate::domain::deployment::{DeploymentEvent, DeploymentStatus, EventMetadata, Platform};
use crate::error::PulsosError;
use crate::platform::{
    AuthStatus, DiscoveredResource, PlatformAdapter, RateLimitInfo, TrackedResource,
};
use chrono::Utc;
use std::sync::Arc;

use super::types::{DeploymentsData, GqlResponse, MeData, ProjectsData, RwDeployment, TeamsData};

pub struct RailwayClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
    cache: Arc<CacheStore>,
}

impl RailwayClient {
    pub fn new(token: String, cache: Arc<CacheStore>) -> Self {
        Self::new_with_base_url(
            token,
            "https://backboard.railway.com/graphql/v2".into(),
            cache,
        )
    }

    pub fn new_with_base_url(token: String, base_url: String, cache: Arc<CacheStore>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("pulsos/0.1.0")
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url,
            token,
            cache,
        }
    }

    async fn execute_query<T: serde::de::DeserializeOwned>(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<T, PulsosError> {
        let body = serde_json::json!({
            "query": query,
            "variables": variables,
        });

        let resp = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "Railway".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(PulsosError::AuthFailed {
                platform: "Railway".into(),
                reason: format!("HTTP {status}"),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(PulsosError::RateLimited {
                platform: "Railway".into(),
                reset_at: "unknown".into(),
                remaining: 0,
            });
        }
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(PulsosError::ApiError {
                platform: "Railway".into(),
                status: status.as_u16(),
                body: body_text,
            });
        }

        let gql_resp: GqlResponse<T> = resp.json().await.map_err(|e| PulsosError::ParseError {
            platform: "Railway".into(),
            message: e.to_string(),
        })?;

        if let Some(errors) = gql_resp.errors {
            if !errors.is_empty() {
                return Err(PulsosError::GraphqlError {
                    platform: "Railway".into(),
                    message: errors
                        .iter()
                        .map(|e| e.message.clone())
                        .collect::<Vec<_>>()
                        .join("; "),
                });
            }
        }

        gql_resp.data.ok_or_else(|| PulsosError::ParseError {
            platform: "Railway".into(),
            message: "GraphQL response had no data and no errors".into(),
        })
    }

    fn deployment_to_event(
        deployment: &RwDeployment,
        service_name: Option<&str>,
        environment_name: Option<&str>,
    ) -> DeploymentEvent {
        DeploymentEvent {
            id: deployment.id.clone(),
            platform: Platform::Railway,
            status: DeploymentStatus::from(deployment.status),
            commit_sha: None, // Railway doesn't expose commit SHAs
            branch: None,
            title: None,
            actor: None,
            created_at: deployment.created_at,
            updated_at: None,
            duration_secs: None,
            url: deployment
                .static_url
                .as_ref()
                .map(|u| format!("https://{u}")),
            metadata: EventMetadata {
                service_name: service_name.map(String::from),
                environment_name: environment_name.map(String::from),
                ..Default::default()
            },
        }
    }
}

const DEPLOYMENTS_QUERY: &str = r#"
query($input: DeploymentListInput!, $first: Int) {
  deployments(input: $input, first: $first) {
    edges {
      node {
        id
        status
        createdAt
        staticUrl
      }
    }
  }
}
"#;

const PROJECTS_QUERY: &str = r#"
query($teamId: String!) {
  projects(teamId: $teamId) {
    edges {
      node {
        id
        name
        description
        createdAt
        services {
          edges {
            node {
              id
              name
            }
          }
        }
        environments {
          edges {
            node {
              id
              name
            }
          }
        }
      }
    }
  }
}
"#;

const ME_QUERY: &str = r#"
query {
  me {
    id
    email
    name
  }
}
"#;

const TEAMS_QUERY: &str = r#"
query {
  teams {
    edges {
      node {
        id
        name
      }
    }
  }
}
"#;

impl PlatformAdapter for RailwayClient {
    async fn fetch_events(
        &self,
        tracked: &[TrackedResource],
    ) -> Result<Vec<DeploymentEvent>, PulsosError> {
        let mut all_events = Vec::new();

        for resource in tracked {
            // resource.platform_id is "projectId:serviceId:environmentId"
            let parts: Vec<&str> = resource.platform_id.split(':').collect();
            if parts.len() < 3 {
                tracing::warn!(
                    resource = %resource.platform_id,
                    "Invalid Railway tracked resource format, expected project:service:environment"
                );
                continue;
            }
            let (project_id, service_id, env_id) = (parts[0], parts[1], parts[2]);

            let variables = serde_json::json!({
                "input": {
                    "projectId": project_id,
                    "serviceId": service_id,
                    "environmentId": env_id,
                },
                "first": 5
            });

            match self
                .execute_query::<DeploymentsData>(DEPLOYMENTS_QUERY, variables)
                .await
            {
                Ok(data) => {
                    let cache_key = crate::cache::keys::railway_deployments_key(project_id);
                    let deployments: Vec<RwDeployment> =
                        data.deployments.edges.into_iter().map(|e| e.node).collect();
                    let _ = self.cache.set(&cache_key, &deployments, 30, None);

                    for d in &deployments {
                        all_events.push(Self::deployment_to_event(
                            d,
                            Some(&resource.display_name),
                            None,
                        ));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        resource = %resource.platform_id,
                        error = %e,
                        "Failed to fetch Railway deployments, trying cache"
                    );
                    let cache_key = crate::cache::keys::railway_deployments_key(project_id);
                    if let Ok(Some(cached)) = self.cache.get::<Vec<RwDeployment>>(&cache_key) {
                        for d in &cached.data {
                            all_events.push(Self::deployment_to_event(
                                d,
                                Some(&resource.display_name),
                                None,
                            ));
                        }
                    }
                }
            }
        }

        all_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(all_events)
    }

    async fn discover(&self) -> Result<Vec<DiscoveredResource>, PulsosError> {
        // First get teams (workspaces)
        let teams_data: TeamsData = self
            .execute_query(TEAMS_QUERY, serde_json::json!({}))
            .await?;

        let mut resources = Vec::new();

        for team_edge in &teams_data.teams.edges {
            let team = &team_edge.node;

            let projects_data: ProjectsData = self
                .execute_query(PROJECTS_QUERY, serde_json::json!({ "teamId": team.id }))
                .await?;

            for proj_edge in &projects_data.projects.edges {
                let proj = &proj_edge.node;
                resources.push(DiscoveredResource {
                    platform_id: proj.id.clone(),
                    display_name: proj.name.clone(),
                    group: team.name.clone(),
                    group_type: "workspace".into(),
                    archived: false,
                    disabled: false,
                });
            }
        }

        Ok(resources)
    }

    async fn validate_auth(&self) -> Result<AuthStatus, PulsosError> {
        let me_data: MeData = self.execute_query(ME_QUERY, serde_json::json!({})).await?;

        let identity = me_data
            .me
            .email
            .unwrap_or_else(|| me_data.me.name.unwrap_or(me_data.me.id));

        Ok(AuthStatus {
            valid: true,
            identity,
            scopes: vec!["account".into()],
            expires_at: None,
            warnings: vec![],
        })
    }

    async fn rate_limit_status(&self) -> Result<RateLimitInfo, PulsosError> {
        // Railway doesn't have documented public rate limits
        Ok(RateLimitInfo {
            limit: 0,
            remaining: 0,
            resets_at: Utc::now(),
            percentage_used: 0.0,
        })
    }
}
