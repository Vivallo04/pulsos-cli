use crate::cache::store::CacheStore;
use crate::domain::deployment::{DeploymentEvent, DeploymentStatus, EventMetadata, Platform};
use crate::error::PulsosError;
use crate::platform::{
    AuthStatus, DiscoveredResource, PlatformAdapter, RateLimitInfo, TrackedResource,
};
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use std::sync::Arc;
use std::time::Duration;

use super::types::{
    DeploymentsResponse, ProjectsResponse, TeamsResponse, VcDeployment, VcUserResponse,
};

pub struct VercelClient {
    client: reqwest::Client,
    base_url: String,
    token: SecretString,
    cache: Arc<CacheStore>,
}

impl VercelClient {
    pub fn new(token: SecretString, cache: Arc<CacheStore>) -> Self {
        Self::new_with_base_url(token, "https://api.vercel.com".into(), cache)
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

    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {}", self.token.expose_secret())
                .parse()
                .unwrap(),
        );
        headers
    }

    async fn parse_json<T: serde::de::DeserializeOwned>(
        &self,
        resp: reqwest::Response,
        endpoint: &str,
    ) -> Result<T, PulsosError> {
        let body_text = resp.text().await.unwrap_or_default();
        serde_json::from_str(&body_text).map_err(|e| {
            tracing::debug!(endpoint, body = %body_text, "Vercel parse failed");
            PulsosError::ParseError {
                platform: "Vercel".into(),
                message: format!(
                    "{endpoint}: {e} — raw: {}",
                    &body_text[..body_text.len().min(240)]
                ),
            }
        })
    }

    async fn fetch_deployments(&self, project_id: &str) -> Result<Vec<VcDeployment>, PulsosError> {
        let req = self
            .client
            .get(format!("{}/v6/deployments", self.base_url))
            .query(&[("projectId", project_id), ("limit", "5")])
            .headers(self.auth_headers());

        let resp = crate::platform::retry::send_with_retry(req, "Vercel").await?;

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

        let body: DeploymentsResponse = self.parse_json(resp, "/v6/deployments").await?;

        Ok(body.deployments)
    }

    fn deployment_to_event(deployment: &VcDeployment, is_from_cache: bool) -> DeploymentEvent {
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
            is_from_cache,
        }
    }

    /// Like `discover()`, but also returns the linked GitHub repo for each project.
    ///
    /// This is used by the `repos sync` command to auto-correlate Vercel → GitHub.
    pub async fn discover_with_links(
        &self,
    ) -> Result<Vec<(DiscoveredResource, Option<String>)>, PulsosError> {
        let mut results = Vec::new();

        let teams_url = format!("{}/v2/teams", self.base_url);
        let req = self.client.get(&teams_url).headers(self.auth_headers());

        let resp = crate::platform::retry::send_with_retry(req, "Vercel").await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PulsosError::ApiError {
                platform: "Vercel".into(),
                status: status.as_u16(),
                body,
            });
        }

        let teams: TeamsResponse = self.parse_json(resp, "/v2/teams").await?;

        for team in &teams.teams {
            let projects_url = format!("{}/v9/projects?teamId={}", self.base_url, team.id);
            let req = self.client.get(&projects_url).headers(self.auth_headers());

            let resp = crate::platform::retry::send_with_retry(req, "Vercel").await?;

            if resp.status().is_success() {
                let projects: ProjectsResponse = self.parse_json(resp, "/v9/projects").await?;

                for proj in &projects.projects {
                    let linked_repo = proj.link.as_ref().and_then(|l| l.repo.clone());
                    results.push((
                        DiscoveredResource {
                            platform_id: proj.id.clone(),
                            display_name: proj.name.clone(),
                            group: team.name.clone(),
                            group_type: "team".into(),
                            archived: false,
                            disabled: false,
                        },
                        linked_repo,
                    ));
                }
            }
        }

        Ok(results)
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
                        all_events.push(Self::deployment_to_event(d, false));
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
                            all_events.push(Self::deployment_to_event(d, true));
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
        let req = self.client.get(&teams_url).headers(self.auth_headers());

        let resp = crate::platform::retry::send_with_retry(req, "Vercel").await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PulsosError::ApiError {
                platform: "Vercel".into(),
                status: status.as_u16(),
                body,
            });
        }

        let teams: TeamsResponse = self.parse_json(resp, "/v2/teams").await?;

        for team in &teams.teams {
            let projects_url = format!("{}/v9/projects?teamId={}", self.base_url, team.id);
            let req = self.client.get(&projects_url).headers(self.auth_headers());

            let resp = crate::platform::retry::send_with_retry(req, "Vercel").await?;

            if resp.status().is_success() {
                let projects: ProjectsResponse = self.parse_json(resp, "/v9/projects").await?;

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
        let req = self.client.get(&url).headers(self.auth_headers());

        let resp = crate::platform::retry::send_with_retry(req, "Vercel").await?;

        if !resp.status().is_success() {
            return Err(PulsosError::AuthFailed {
                platform: "Vercel".into(),
                reason: format!("HTTP {}", resp.status()),
            });
        }

        let body_text = resp.text().await.unwrap_or_default();
        let user_resp: VcUserResponse = serde_json::from_str(&body_text).map_err(|e| {
            tracing::debug!(body = %body_text, "Vercel /v2/user parse failed");
            PulsosError::ParseError {
                platform: "Vercel".into(),
                message: format!("{e} — raw: {}", &body_text[..body_text.len().min(200)]),
            }
        })?;

        let user = user_resp.into_user();
        let identity = user.username.or(user.name).unwrap_or(user.uid);

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
