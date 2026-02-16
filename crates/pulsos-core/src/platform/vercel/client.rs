use crate::cache::store::CacheStore;
use crate::domain::deployment::{DeploymentEvent, DeploymentStatus, EventMetadata, Platform};
use crate::error::PulsosError;
use crate::platform::{
    AuthStatus, DiscoveredResource, PlatformAdapter, RateLimitInfo, TrackedResource,
};
use chrono::{DateTime, Utc};
use std::sync::Arc;

use super::types::{
    DeploymentsResponse, ProjectsResponse, TeamsResponse, VcDeployment, VcUserResponse,
};

pub struct VercelClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
    cache: Arc<CacheStore>,
}

impl VercelClient {
    pub fn new(token: String, cache: Arc<CacheStore>) -> Self {
        Self::new_with_base_url(token, "https://api.vercel.com".into(), cache)
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

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {}", self.token).parse().unwrap(),
        );
        headers
    }

    async fn fetch_deployments(&self, project_id: &str) -> Result<Vec<VcDeployment>, PulsosError> {
        let resp = self
            .client
            .get(format!("{}/v6/deployments", self.base_url))
            .query(&[("projectId", project_id), ("limit", "5")])
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "Vercel".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(PulsosError::AuthFailed {
                platform: "Vercel".into(),
                reason: format!("HTTP {status}"),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(PulsosError::RateLimited {
                platform: "Vercel".into(),
                reset_at: "unknown".into(),
                remaining: 0,
            });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PulsosError::ApiError {
                platform: "Vercel".into(),
                status: status.as_u16(),
                body,
            });
        }

        let body: DeploymentsResponse = resp.json().await.map_err(|e| PulsosError::ParseError {
            platform: "Vercel".into(),
            message: e.to_string(),
        })?;

        Ok(body.deployments)
    }

    fn deployment_to_event(deployment: &VcDeployment) -> DeploymentEvent {
        let status = deployment
            .ready_state
            .or(deployment.state)
            .map(DeploymentStatus::from)
            .unwrap_or(DeploymentStatus::Unknown("no_state".into()));

        let (commit_sha, branch, title, actor) = match &deployment.meta {
            Some(meta) => (
                meta.github_commit_sha.clone(),
                meta.github_commit_ref.clone(),
                meta.github_commit_message.clone(),
                meta.github_commit_author_name.clone(),
            ),
            None => (None, None, None, None),
        };

        // Vercel `created` is Unix timestamp in milliseconds
        let created_at =
            DateTime::from_timestamp_millis(deployment.created as i64).unwrap_or_else(Utc::now);

        let ready_at = deployment
            .ready
            .and_then(|ts| DateTime::from_timestamp_millis(ts as i64));

        let duration_secs = deployment.building_at.and_then(|build_start| {
            deployment.ready.map(|ready| {
                let diff = ready.saturating_sub(build_start);
                diff / 1000 // ms to seconds
            })
        });

        DeploymentEvent {
            id: deployment.uid.clone(),
            platform: Platform::Vercel,
            status,
            commit_sha,
            branch,
            title,
            actor,
            created_at,
            updated_at: ready_at,
            duration_secs,
            url: deployment.url.as_ref().map(|u| format!("https://{u}")),
            metadata: EventMetadata {
                preview_url: deployment.url.clone(),
                deploy_target: deployment.target.clone(),
                ..Default::default()
            },
        }
    }
}

impl PlatformAdapter for VercelClient {
    async fn fetch_events(
        &self,
        tracked: &[TrackedResource],
    ) -> Result<Vec<DeploymentEvent>, PulsosError> {
        let mut all_events = Vec::new();

        for resource in tracked {
            match self.fetch_deployments(&resource.platform_id).await {
                Ok(deployments) => {
                    let cache_key =
                        crate::cache::keys::vercel_deployments_key(&resource.platform_id);
                    let _ = self.cache.set(&cache_key, &deployments, 30, None);

                    for d in &deployments {
                        all_events.push(Self::deployment_to_event(d));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        project = %resource.platform_id,
                        error = %e,
                        "Failed to fetch Vercel deployments, trying cache"
                    );
                    let cache_key =
                        crate::cache::keys::vercel_deployments_key(&resource.platform_id);
                    if let Ok(Some(cached)) = self.cache.get::<Vec<VcDeployment>>(&cache_key) {
                        for d in &cached.data {
                            all_events.push(Self::deployment_to_event(d));
                        }
                    }
                }
            }
        }

        all_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(all_events)
    }

    async fn discover(&self) -> Result<Vec<DiscoveredResource>, PulsosError> {
        let mut resources = Vec::new();

        // Fetch teams
        let teams_url = format!("{}/v2/teams", self.base_url);
        let resp = self
            .client
            .get(&teams_url)
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "Vercel".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PulsosError::ApiError {
                platform: "Vercel".into(),
                status: status.as_u16(),
                body,
            });
        }

        let teams: TeamsResponse = resp.json().await.map_err(|e| PulsosError::ParseError {
            platform: "Vercel".into(),
            message: e.to_string(),
        })?;

        for team in &teams.teams {
            let projects_url = format!("{}/v9/projects?teamId={}", self.base_url, team.id);
            let resp = self
                .client
                .get(&projects_url)
                .headers(self.auth_headers())
                .send()
                .await
                .map_err(|e| PulsosError::Network {
                    platform: "Vercel".into(),
                    message: e.to_string(),
                    source: Some(e),
                })?;

            if resp.status().is_success() {
                let projects: ProjectsResponse =
                    resp.json().await.map_err(|e| PulsosError::ParseError {
                        platform: "Vercel".into(),
                        message: e.to_string(),
                    })?;

                for proj in &projects.projects {
                    resources.push(DiscoveredResource {
                        platform_id: proj.id.clone(),
                        display_name: proj.name.clone(),
                        group: team.name.clone(),
                        group_type: "team".into(),
                        archived: false,
                        disabled: false,
                    });
                }
            }
        }

        Ok(resources)
    }

    async fn validate_auth(&self) -> Result<AuthStatus, PulsosError> {
        let url = format!("{}/v2/user", self.base_url);
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "Vercel".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        if !resp.status().is_success() {
            return Err(PulsosError::AuthFailed {
                platform: "Vercel".into(),
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let user_resp: VcUserResponse = resp.json().await.map_err(|e| PulsosError::ParseError {
            platform: "Vercel".into(),
            message: e.to_string(),
        })?;

        let identity = user_resp
            .user
            .username
            .or(user_resp.user.name)
            .unwrap_or(user_resp.user.uid);

        Ok(AuthStatus {
            valid: true,
            identity,
            scopes: vec![],
            expires_at: None,
            warnings: vec![],
        })
    }

    async fn rate_limit_status(&self) -> Result<RateLimitInfo, PulsosError> {
        // Vercel rate limits vary by plan and aren't well-documented
        Ok(RateLimitInfo {
            limit: 0,
            remaining: 0,
            resets_at: Utc::now(),
            percentage_used: 0.0,
        })
    }
}
