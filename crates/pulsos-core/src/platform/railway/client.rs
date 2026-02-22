use crate::cache::store::CacheStore;
use crate::domain::deployment::{DeploymentEvent, DeploymentStatus, EventMetadata, Platform};
use crate::error::PulsosError;
use crate::platform::{
    AuthStatus, DiscoveredResource, PlatformAdapter, RateLimitInfo, TrackedResource,
};
use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;
use std::time::Duration;

use super::types::{
    DeploymentsData, GqlResponse, MeData, MetricsData, ProjectsData, RailwayResourceIds,
    RwDeployment, TeamsData, WorkspaceData, WorkspaceProjectsData,
};

pub struct RailwayClient {
    client: reqwest::Client,
    base_url: String,
    token: SecretString,
    cache: Arc<CacheStore>,
}

impl RailwayClient {
    pub fn new(token: SecretString, cache: Arc<CacheStore>) -> Self {
        Self::new_with_base_url(
            token,
            "https://backboard.railway.com/graphql/v2".into(),
            cache,
        )
    }

    pub fn new_with_base_url(
        token: SecretString,
        base_url: String,
        cache: Arc<CacheStore>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("pulsos/0.1.0")
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
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

        let req = self
            .client
            .post(&self.base_url)
            .header(
                "Authorization",
                format!("Bearer {}", self.token.expose_secret()),
            )
            .header("Content-Type", "application/json")
            .json(&body);

        let resp = crate::platform::retry::send_with_retry(req, "Railway").await?;

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
                let message = errors
                    .iter()
                    .map(|e| e.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ");

                // "Not Authorized" at the GraphQL level almost always means a
                // Workspace or Project token was used instead of an Account token.
                let message = if message.to_ascii_lowercase().contains("not authorized") {
                    format!(
                        "{message} — use an Account token (not a Workspace or Project token). \
                         Create one at https://railway.com/account/tokens"
                    )
                } else {
                    message
                };

                return Err(PulsosError::GraphqlError {
                    platform: "Railway".into(),
                    message,
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
        source_id: Option<&str>,
        is_from_cache: bool,
    ) -> DeploymentEvent {
        // Extract commit info from meta if available
        let (commit_sha, branch, meta_message, actor) = deployment
            .meta
            .as_ref()
            .map(|m| {
                (
                    m.get("commitHash")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    m.get("branch")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    m.get("commitMessage")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    m.get("commitAuthor")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                )
            })
            .unwrap_or((None, None, None, None));

        // Title: prefer commit message, fall back to service name
        let title = meta_message.or_else(|| service_name.map(String::from));

        // Duration from createdAt -> updatedAt
        let duration_secs = deployment.updated_at.map(|updated| {
            (updated - deployment.created_at).num_seconds().max(0) as u64
        });

        DeploymentEvent {
            id: deployment.id.clone(),
            platform: Platform::Railway,
            status: DeploymentStatus::from(deployment.status),
            commit_sha,
            branch,
            title,
            actor,
            created_at: deployment.created_at,
            updated_at: deployment.updated_at,
            duration_secs,
            url: deployment
                .static_url
                .as_ref()
                .map(|u| format!("https://{u}")),
            metadata: EventMetadata {
                service_name: service_name.map(String::from),
                environment_name: environment_name.map(String::from),
                source_id: source_id.map(String::from),
                ..Default::default()
            },
            is_from_cache,
        }
    }

    /// Fetch real-time container metrics for a Railway service.
    ///
    /// `platform_id` uses the format `"projectId:serviceId:environmentId"` stored in
    /// `TrackedResource.platform_id`. Returns an empty `ResourceMetrics` (all `None`) if the
    /// format is invalid or the API call fails, so callers degrade gracefully.
    pub async fn fetch_service_metrics(
        &self,
        platform_id: &str,
    ) -> crate::domain::metrics::ResourceMetrics {
        let ids = match RailwayResourceIds::from_platform_id(platform_id) {
            Some(ids) => ids,
            None => {
                tracing::warn!(platform_id, "Invalid Railway platform_id format for metrics");
                return crate::domain::metrics::ResourceMetrics::default();
            }
        };

        let start_date = (chrono::Utc::now() - chrono::Duration::minutes(5))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let variables = serde_json::json!({
            "environmentId": ids.environment_id,
            "serviceId": ids.service_id,
            "startDate": start_date,
            "measurements": [
                "CPU_USAGE",
                "MEMORY_USAGE_GB",
                "MEMORY_LIMIT_GB",
                "NETWORK_RX_GB",
                "NETWORK_TX_GB",
            ],
        });

        let data: MetricsData = match self
            .execute_query(METRICS_QUERY, variables)
            .await
        {
            Ok(d) => d,
            Err(e) => {
                tracing::debug!(
                    platform_id,
                    error = %e,
                    "Railway metrics fetch failed"
                );
                return crate::domain::metrics::ResourceMetrics::default();
            }
        };

        let mut metrics = crate::domain::metrics::ResourceMetrics::default();

        for series in &data.metrics {
            let latest_value = series.values.last().map(|p| p.value);
            if let Some(val) = latest_value {
                match series.measurement.as_str() {
                    "CPU_USAGE" => metrics.cpu_percent = Some(val * 100.0),
                    "MEMORY_USAGE_GB" => metrics.memory_used_mb = Some(val * 1024.0),
                    "MEMORY_LIMIT_GB" => metrics.memory_limit_mb = Some(val * 1024.0),
                    "NETWORK_RX_GB" => {
                        metrics.network_rx_bytes = Some((val * 1_073_741_824.0) as u64)
                    }
                    "NETWORK_TX_GB" => {
                        metrics.network_tx_bytes = Some((val * 1_073_741_824.0) as u64)
                    }
                    _ => {}
                }
            }
        }

        metrics.timestamp = chrono::Utc::now();
        metrics
    }

    async fn discover_legacy_by_teams(&self) -> Result<Vec<DiscoveredResource>, PulsosError> {
        let teams_data: TeamsData = self
            .execute_query(TEAMS_QUERY, serde_json::json!({}))
            .await?;
        let mut resources = Vec::new();

        for team_edge in &teams_data.teams.edges {
            let team = &team_edge.node;
            let projects_data: ProjectsData = self
                .execute_query(
                    LEGACY_PROJECTS_QUERY,
                    serde_json::json!({ "teamId": team.id }),
                )
                .await?;

            for proj_edge in &projects_data.projects.edges {
                let proj = &proj_edge.node;
                let services: Vec<_> = proj.services.edges.iter().map(|s| &s.node).collect();
                let environments: Vec<_> =
                    proj.environments.edges.iter().map(|e| &e.node).collect();

                if services.is_empty() || environments.is_empty() {
                    continue;
                }

                for service in &services {
                    for environment in &environments {
                        resources.push(DiscoveredResource {
                            platform_id: format!("{}:{}:{}", proj.id, service.id, environment.id),
                            display_name: format!(
                                "{} / {} / {}",
                                proj.name, service.name, environment.name
                            ),
                            group: team.name.clone(),
                            group_type: "workspace".into(),
                            archived: false,
                            disabled: false,
                        });
                    }
                }
            }
        }

        Ok(resources)
    }
}

const METRICS_QUERY: &str = r#"
query($environmentId: String!, $serviceId: String, $startDate: DateTime!, $measurements: [MetricMeasurement!]!) {
  metrics(environmentId: $environmentId, serviceId: $serviceId, startDate: $startDate, measurements: $measurements) {
    measurement
    values {
      ts
      value
    }
  }
}
"#;

const DEPLOYMENTS_QUERY: &str = r#"
query($input: DeploymentListInput!, $first: Int) {
  deployments(input: $input, first: $first) {
    edges {
      node {
        id
        status
        createdAt
        updatedAt
        staticUrl
        meta
      }
    }
  }
}
"#;

const PROJECTS_QUERY: &str = r#"
query {
  projects {
    edges {
      node {
        id
        name
        workspaceId
        workspace {
          id
          name
        }
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

const LEGACY_PROJECTS_QUERY: &str = r#"
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

const WORKSPACE_QUERY: &str = r#"
query($workspaceId: String!) {
  workspace(workspaceId: $workspaceId) {
    id
    name
  }
}
"#;

const WORKSPACE_PROJECTS_QUERY: &str = r#"
query($workspaceId: String!) {
  workspace(workspaceId: $workspaceId) {
    id
    name
    projects {
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

const ME_QUERY: &str = r#"
query {
  me {
    id
    email
    name
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
                    let cache_key =
                        crate::cache::keys::railway_deployments_key(project_id, service_id, env_id);
                    let deployments: Vec<RwDeployment> =
                        data.deployments.edges.into_iter().map(|e| e.node).collect();
                    let _ = self.cache.set(&cache_key, &deployments, 30, None);

                    for d in &deployments {
                        all_events.push(Self::deployment_to_event(
                            d,
                            Some(&resource.display_name),
                            None,
                            Some(&resource.platform_id),
                            false,
                        ));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        resource = %resource.platform_id,
                        error = %e,
                        "Failed to fetch Railway deployments, trying cache"
                    );
                    let cache_key =
                        crate::cache::keys::railway_deployments_key(project_id, service_id, env_id);
                    if let Ok(Some(cached)) = self.cache.get::<Vec<RwDeployment>>(&cache_key) {
                        for d in &cached.data {
                            all_events.push(Self::deployment_to_event(
                                d,
                                Some(&resource.display_name),
                                None,
                                Some(&resource.platform_id),
                                true,
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
        let mut workspace_names: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut resources = Vec::new();
        let projects_data: ProjectsData = self
            .execute_query(PROJECTS_QUERY, serde_json::json!({}))
            .await?;

        // Some account tokens return an empty top-level `projects` list but allow
        // project access through `workspace(workspaceId: ...).projects`.
        if projects_data.projects.edges.is_empty() {
            if let Ok(me_data) = self
                .execute_query::<MeData>(ME_QUERY, serde_json::json!({}))
                .await
            {
                if let Ok(workspace_projects) = self
                    .execute_query::<WorkspaceProjectsData>(
                        WORKSPACE_PROJECTS_QUERY,
                        serde_json::json!({ "workspaceId": me_data.me.id }),
                    )
                    .await
                {
                    if let Some(workspace) = workspace_projects.workspace {
                        for proj_edge in &workspace.projects.edges {
                            let proj = &proj_edge.node;
                            let services: Vec<_> =
                                proj.services.edges.iter().map(|s| &s.node).collect();
                            let environments: Vec<_> =
                                proj.environments.edges.iter().map(|e| &e.node).collect();

                            if services.is_empty() || environments.is_empty() {
                                continue;
                            }

                            for service in &services {
                                for environment in &environments {
                                    resources.push(DiscoveredResource {
                                        platform_id: format!(
                                            "{}:{}:{}",
                                            proj.id, service.id, environment.id
                                        ),
                                        display_name: format!(
                                            "{} / {} / {}",
                                            proj.name, service.name, environment.name
                                        ),
                                        group: workspace.name.clone(),
                                        group_type: "workspace".into(),
                                        archived: false,
                                        disabled: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        if resources.is_empty() {
            for proj_edge in &projects_data.projects.edges {
                let proj = &proj_edge.node;
                let services: Vec<_> = proj.services.edges.iter().map(|s| &s.node).collect();
                let environments: Vec<_> =
                    proj.environments.edges.iter().map(|e| &e.node).collect();

                if services.is_empty() || environments.is_empty() {
                    tracing::debug!(
                        project_id = %proj.id,
                        project_name = %proj.name,
                        "Skipping Railway project with missing services or environments"
                    );
                    continue;
                }

                let workspace_name = if let Some(workspace) = proj.workspace.as_ref() {
                    workspace.name.clone()
                } else if let Some(workspace_id) = proj.workspace_id.as_ref() {
                    if let Some(name) = workspace_names.get(workspace_id) {
                        name.clone()
                    } else {
                        let query_result: Result<WorkspaceData, PulsosError> = self
                            .execute_query(
                                WORKSPACE_QUERY,
                                serde_json::json!({ "workspaceId": workspace_id }),
                            )
                            .await;
                        match query_result {
                            Ok(data) => {
                                let name = data
                                    .workspace
                                    .as_ref()
                                    .map(|w| w.name.clone())
                                    .unwrap_or_else(|| workspace_id.clone());
                                workspace_names.insert(workspace_id.clone(), name.clone());
                                name
                            }
                            Err(_) => workspace_id.clone(),
                        }
                    }
                } else {
                    "default".to_string()
                };

                for service in &services {
                    for environment in &environments {
                        resources.push(DiscoveredResource {
                            platform_id: format!("{}:{}:{}", proj.id, service.id, environment.id),
                            display_name: format!(
                                "{} / {} / {}",
                                proj.name, service.name, environment.name
                            ),
                            group: workspace_name.clone(),
                            group_type: "workspace".into(),
                            archived: false,
                            disabled: false,
                        });
                    }
                }
            }
        }

        if resources.iter().any(|r| r.group == "default") {
            match self.discover_legacy_by_teams().await {
                Ok(legacy) if !legacy.is_empty() => return Ok(legacy),
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!(error = %e, "Railway legacy team discovery fallback failed");
                }
            }
        }

        Ok(resources)
    }

    async fn validate_auth(&self) -> Result<AuthStatus, PulsosError> {
        match self
            .execute_query::<MeData>(ME_QUERY, serde_json::json!({}))
            .await
        {
            Ok(me_data) => {
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
            Err(me_err) => {
                // Some Railway token types cannot query `me` but still access projects.
                // Accept these tokens with a warning so discovery/status can proceed.
                let projects_result: Result<ProjectsData, PulsosError> = self
                    .execute_query(PROJECTS_QUERY, serde_json::json!({}))
                    .await;

                match projects_result {
                    Ok(_) => Ok(AuthStatus {
                        valid: true,
                        identity: "railway (project/workspace token)".to_string(),
                        scopes: vec!["project".into()],
                        expires_at: None,
                        warnings: vec![format!(
                            "Token cannot query `me` ({}). This is likely a Project/Workspace token. \
Use an Account token for full API access: https://railway.com/account/tokens",
                            me_err
                        )],
                    }),
                    Err(_) => Err(me_err),
                }
            }
        }
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
