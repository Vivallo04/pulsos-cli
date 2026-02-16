use crate::cache::store::CacheStore;
use crate::domain::deployment::{DeploymentEvent, EventMetadata, Platform};
use crate::error::PulsosError;
use crate::platform::{
    AuthStatus, DiscoveredResource, PlatformAdapter, RateLimitInfo, TrackedResource,
};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::{GhRateLimit, GhRepo, GhUser, WorkflowRun, WorkflowRunsResponse};

pub struct GitHubClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
    cache: Arc<CacheStore>,
    rate_limit: RwLock<Option<GhRateLimit>>,
}

impl GitHubClient {
    pub fn new(token: String, cache: Arc<CacheStore>) -> Self {
        Self::new_with_base_url(token, "https://api.github.com".into(), cache)
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
            rate_limit: RwLock::new(None),
        }
    }

    fn auth_headers(&self) -> Result<HeaderMap, PulsosError> {
        let mut headers = HeaderMap::new();
        let auth = HeaderValue::from_str(&format!("Bearer {}", self.token)).map_err(|e| {
            PulsosError::AuthFailed {
                platform: "GitHub".into(),
                reason: format!("Invalid token format for Authorization header: {e}"),
            }
        })?;
        headers.insert(AUTHORIZATION, auth);
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        Ok(headers)
    }

    async fn update_rate_limit(&self, resp: &reqwest::Response) {
        let headers = resp.headers();
        let parse_header = |name: &str| -> Option<u32> {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
        };

        let limit = parse_header("x-ratelimit-limit");
        let remaining = parse_header("x-ratelimit-remaining");
        let used = parse_header("x-ratelimit-used");
        let reset_ts = headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .and_then(|ts| DateTime::from_timestamp(ts, 0));

        if let (Some(limit), Some(remaining), Some(reset)) = (limit, remaining, reset_ts) {
            let mut rl = self.rate_limit.write().await;
            *rl = Some(GhRateLimit {
                limit,
                remaining,
                reset,
                used: used.unwrap_or(0),
            });
        }
    }

    async fn fetch_workflow_runs(&self, repo: &str) -> Result<Vec<WorkflowRun>, PulsosError> {
        let url = format!("{}/repos/{}/actions/runs?per_page=5", self.base_url, repo);

        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers()?)
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "GitHub".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        self.update_rate_limit(&resp).await;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(PulsosError::AuthFailed {
                platform: "GitHub".into(),
                reason: format!("HTTP {status}"),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let rl = self.rate_limit.read().await;
            let reset_at = rl
                .as_ref()
                .map(|r| r.reset.to_rfc3339())
                .unwrap_or_else(|| "unknown".into());
            return Err(PulsosError::RateLimited {
                platform: "GitHub".into(),
                reset_at,
                remaining: 0,
            });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PulsosError::ApiError {
                platform: "GitHub".into(),
                status: status.as_u16(),
                body,
            });
        }

        let body: WorkflowRunsResponse =
            resp.json().await.map_err(|e| PulsosError::ParseError {
                platform: "GitHub".into(),
                message: e.to_string(),
            })?;

        Ok(body.workflow_runs)
    }

    fn run_to_event(run: &WorkflowRun, _repo: &str) -> DeploymentEvent {
        let duration = match (run.run_started_at, run.updated_at) {
            (Some(start), end) => {
                let diff = end - start;
                Some(diff.num_seconds().max(0) as u64)
            }
            _ => None,
        };

        DeploymentEvent {
            id: run.id.to_string(),
            platform: Platform::GitHub,
            status: run.to_deployment_status(),
            commit_sha: Some(run.head_sha.clone()),
            branch: run.head_branch.clone(),
            title: run.display_title.clone().or_else(|| run.name.clone()),
            actor: run.actor.as_ref().map(|a| a.login.clone()),
            created_at: run.created_at,
            updated_at: Some(run.updated_at),
            duration_secs: duration,
            url: Some(run.html_url.clone()),
            metadata: EventMetadata {
                workflow_name: run.name.clone(),
                trigger_event: Some(run.event.clone()),
                ..Default::default()
            },
        }
    }
}

impl PlatformAdapter for GitHubClient {
    async fn fetch_events(
        &self,
        tracked: &[TrackedResource],
    ) -> Result<Vec<DeploymentEvent>, PulsosError> {
        let mut all_events = Vec::new();

        for resource in tracked {
            match self.fetch_workflow_runs(&resource.platform_id).await {
                Ok(runs) => {
                    // Cache the results
                    let cache_key = crate::cache::keys::github_runs_key(&resource.platform_id);
                    let _ = self.cache.set(&cache_key, &runs, 30, None);

                    for run in &runs {
                        all_events.push(Self::run_to_event(run, &resource.platform_id));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        repo = %resource.platform_id,
                        error = %e,
                        "Failed to fetch GitHub runs, trying cache"
                    );
                    // Try cache fallback
                    let cache_key = crate::cache::keys::github_runs_key(&resource.platform_id);
                    if let Ok(Some(cached)) = self.cache.get::<Vec<WorkflowRun>>(&cache_key) {
                        for run in &cached.data {
                            all_events.push(Self::run_to_event(run, &resource.platform_id));
                        }
                    }
                }
            }
        }

        // Sort by created_at descending
        all_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(all_events)
    }

    async fn discover(&self) -> Result<Vec<DiscoveredResource>, PulsosError> {
        let mut resources = Vec::new();

        // Fetch user's repos
        let url = format!("{}/user/repos?per_page=100&type=owner", self.base_url);
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers()?)
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "GitHub".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        self.update_rate_limit(&resp).await;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PulsosError::ApiError {
                platform: "GitHub".into(),
                status: status.as_u16(),
                body,
            });
        }

        let repos: Vec<GhRepo> = resp.json().await.map_err(|e| PulsosError::ParseError {
            platform: "GitHub".into(),
            message: e.to_string(),
        })?;

        for repo in repos {
            let group_type = if repo.owner.owner_type.eq_ignore_ascii_case("organization") {
                "organization"
            } else {
                "user"
            };

            resources.push(DiscoveredResource {
                platform_id: repo.full_name.clone(),
                display_name: repo.name,
                group: repo.owner.login,
                group_type: group_type.into(),
                archived: repo.archived,
                disabled: repo.disabled,
            });
        }

        Ok(resources)
    }

    async fn validate_auth(&self) -> Result<AuthStatus, PulsosError> {
        let url = format!("{}/user", self.base_url);
        let resp = self
            .client
            .get(&url)
            .headers(self.auth_headers()?)
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "GitHub".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        self.update_rate_limit(&resp).await;

        if !resp.status().is_success() {
            return Err(PulsosError::AuthFailed {
                platform: "GitHub".into(),
                reason: format!("HTTP {}", resp.status()),
            });
        }

        // Extract scopes from response header
        let scopes = resp
            .headers()
            .get("x-oauth-scopes")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(", ").map(String::from).collect::<Vec<_>>())
            .unwrap_or_default();

        let user: GhUser = resp.json().await.map_err(|e| PulsosError::ParseError {
            platform: "GitHub".into(),
            message: e.to_string(),
        })?;

        Ok(AuthStatus {
            valid: true,
            identity: format!("@{}", user.login),
            scopes,
            expires_at: None,
            warnings: vec![],
        })
    }

    async fn rate_limit_status(&self) -> Result<RateLimitInfo, PulsosError> {
        let rl = self.rate_limit.read().await;
        match rl.as_ref() {
            Some(rl) => Ok(RateLimitInfo {
                limit: rl.limit,
                remaining: rl.remaining,
                resets_at: rl.reset,
                percentage_used: if rl.limit > 0 {
                    (rl.limit - rl.remaining) as f32 / rl.limit as f32 * 100.0
                } else {
                    0.0
                },
            }),
            None => Ok(RateLimitInfo {
                limit: 5000,
                remaining: 5000,
                resets_at: Utc::now(),
                percentage_used: 0.0,
            }),
        }
    }
}
