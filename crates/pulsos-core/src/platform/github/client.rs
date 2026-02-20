use crate::cache::store::CacheStore;
use crate::domain::deployment::{DeploymentEvent, EventMetadata, Platform};
use crate::error::PulsosError;
use crate::platform::{
    AuthStatus, DiscoveredResource, PlatformAdapter, RateLimitInfo, TrackedResource,
};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, ETAG, IF_NONE_MATCH};
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::types::{
    GhCollaboratorPermission, GhOrg, GhRateLimit, GhRepo, GhUser, WorkflowRun, WorkflowRunsResponse,
};

pub struct GitHubClient {
    client: reqwest::Client,
    base_url: String,
    token: SecretString,
    cache: Arc<CacheStore>,
    rate_limit: RwLock<Option<GhRateLimit>>,
}

impl GitHubClient {
    pub fn new(token: SecretString, cache: Arc<CacheStore>) -> Self {
        Self::new_with_base_url(token, "https://api.github.com".into(), cache)
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
            rate_limit: RwLock::new(None),
        }
    }

    fn auth_headers(&self) -> Result<HeaderMap, PulsosError> {
        let mut headers = HeaderMap::new();
        let auth = HeaderValue::from_str(&format!("Bearer {}", self.token.expose_secret()))
            .map_err(|e| PulsosError::AuthFailed {
                platform: "GitHub".into(),
                reason: format!("Invalid token format for Authorization header: {e}"),
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

    /// Returns `true` when the remaining GitHub API budget is below 10%.
    /// When the budget is unknown (no previous request has been made), returns `false`.
    async fn is_rate_limit_low(&self) -> bool {
        let rl = self.rate_limit.read().await;
        match rl.as_ref() {
            Some(rl) if rl.limit > 0 => (rl.remaining as f32 / rl.limit as f32) < 0.10,
            _ => false,
        }
    }

    async fn fetch_workflow_runs(&self, repo: &str) -> Result<Vec<WorkflowRun>, PulsosError> {
        let url = format!("{}/repos/{}/actions/runs?per_page=5", self.base_url, repo);
        let cache_key = crate::cache::keys::github_runs_key(repo);

        // Serve from cache immediately when rate-limit budget is critically low.
        if self.is_rate_limit_low().await {
            if let Ok(Some(cached)) = self.cache.get::<Vec<WorkflowRun>>(&cache_key) {
                tracing::warn!(repo = %repo, "GitHub rate limit < 10%, serving from cache");
                return Ok(cached.data);
            }
        }

        // Read cached ETag to send as If-None-Match
        let cached_etag = self
            .cache
            .get::<Vec<WorkflowRun>>(&cache_key)
            .ok()
            .flatten()
            .and_then(|e| e.etag);

        let mut headers = self.auth_headers()?;
        if let Some(ref etag) = cached_etag {
            if let Ok(val) = HeaderValue::from_str(etag) {
                headers.insert(IF_NONE_MATCH, val);
            }
        }

        let resp = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "GitHub".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        self.update_rate_limit(&resp).await;

        let status = resp.status();

        // 304 Not Modified — data unchanged, serve from cache
        if status == reqwest::StatusCode::NOT_MODIFIED {
            if let Ok(Some(cached)) = self.cache.get::<Vec<WorkflowRun>>(&cache_key) {
                return Ok(cached.data);
            }
            // Cache miss despite 304 — treat as a fresh fetch error
            return Err(PulsosError::ApiError {
                platform: "GitHub".into(),
                status: 304,
                body: "304 Not Modified but no cached data available".into(),
            });
        }

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

        // Extract ETag before consuming the response body
        let new_etag = resp
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let body: WorkflowRunsResponse =
            resp.json().await.map_err(|e| PulsosError::ParseError {
                platform: "GitHub".into(),
                message: e.to_string(),
            })?;

        // Cache the fresh result with its ETag
        let _ = self
            .cache
            .set(&cache_key, &body.workflow_runs, 30, new_etag);

        Ok(body.workflow_runs)
    }

    /// Returns the login name of the authenticated user.
    pub async fn fetch_user_login(&self) -> Result<String, PulsosError> {
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

        let user: GhUser = resp.json().await.map_err(|e| PulsosError::ParseError {
            platform: "GitHub".into(),
            message: e.to_string(),
        })?;

        Ok(user.login)
    }

    /// Checks the permission level of `user` on `owner/repo`.
    /// Returns the permission string: "admin", "write", "read", or "none".
    pub async fn check_repo_permission(
        &self,
        owner: &str,
        repo: &str,
        user: &str,
    ) -> Result<String, PulsosError> {
        let url = format!(
            "{}/repos/{}/{}/collaborators/{}/permission",
            self.base_url, owner, repo, user
        );
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
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PulsosError::ApiError {
                platform: "GitHub".into(),
                status: status.as_u16(),
                body,
            });
        }

        let perm: GhCollaboratorPermission =
            resp.json().await.map_err(|e| PulsosError::ParseError {
                platform: "GitHub".into(),
                message: e.to_string(),
            })?;

        Ok(perm.permission)
    }

    fn run_to_event(run: &WorkflowRun, repo: &str, is_from_cache: bool) -> DeploymentEvent {
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
                source_id: Some(repo.to_string()),
                ..Default::default()
            },
            is_from_cache,
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
                    for run in &runs {
                        all_events.push(Self::run_to_event(run, &resource.platform_id, false));
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
                            all_events.push(Self::run_to_event(run, &resource.platform_id, true));
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
        let mut seen: HashSet<String> = HashSet::new();

        // Fetch user's own repos
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
            seen.insert(repo.full_name.clone());
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

        // Fetch orgs and discover repos in each org namespace
        let orgs_url = format!("{}/user/orgs?per_page=100", self.base_url);
        let orgs_resp = self
            .client
            .get(&orgs_url)
            .headers(self.auth_headers()?)
            .send()
            .await
            .map_err(|e| PulsosError::Network {
                platform: "GitHub".into(),
                message: e.to_string(),
                source: Some(e),
            })?;

        self.update_rate_limit(&orgs_resp).await;

        let orgs_status = orgs_resp.status();
        if orgs_status.is_success() {
            let orgs: Vec<GhOrg> = orgs_resp.json().await.unwrap_or_default();

            for org in orgs {
                let org_repos_url = format!(
                    "{}/orgs/{}/repos?per_page=100&type=all",
                    self.base_url, org.login
                );
                match self
                    .client
                    .get(&org_repos_url)
                    .headers(self.auth_headers()?)
                    .send()
                    .await
                {
                    Ok(resp) => {
                        self.update_rate_limit(&resp).await;
                        let repo_status = resp.status();
                        if repo_status.is_success() {
                            match resp.json::<Vec<GhRepo>>().await {
                                Ok(org_repos) => {
                                    for repo in org_repos {
                                        if seen.insert(repo.full_name.clone()) {
                                            resources.push(DiscoveredResource {
                                                platform_id: repo.full_name.clone(),
                                                display_name: repo.name,
                                                group: repo.owner.login,
                                                group_type: "organization".into(),
                                                archived: repo.archived,
                                                disabled: repo.disabled,
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(org = %org.login, error = %e, "Failed to parse org repos");
                                }
                            }
                        } else if repo_status == reqwest::StatusCode::FORBIDDEN
                            || repo_status == reqwest::StatusCode::UNAUTHORIZED
                        {
                            tracing::warn!(
                                org = %org.login,
                                "Insufficient permissions to list org repos (token needs read:org scope)"
                            );
                        } else {
                            tracing::warn!(org = %org.login, status = %repo_status, "Failed to fetch org repos");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(org = %org.login, error = %e, "Network error fetching org repos");
                    }
                }
            }
        } else {
            tracing::warn!(
                status = %orgs_status,
                "Failed to fetch user orgs (token may need read:org scope)"
            );
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
