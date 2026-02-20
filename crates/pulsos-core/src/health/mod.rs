use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::auth::resolve::TokenResolver;
use crate::auth::PlatformKind;
use crate::cache::store::CacheStore;
use crate::config::types::PulsosConfig;
use crate::error::PulsosError;
use crate::platform::github::client::GitHubClient;
use crate::platform::railway::client::RailwayClient;
use crate::platform::vercel::client::VercelClient;
use crate::platform::PlatformAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformHealthState {
    NoToken,
    InvalidToken,
    ConnectivityError,
    AccessOrConfigIncomplete,
    Ready,
}

impl PlatformHealthState {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoToken => "No Token",
            Self::InvalidToken => "Invalid Token",
            Self::ConnectivityError => "Connectivity Error",
            Self::AccessOrConfigIncomplete => "Needs Config",
            Self::Ready => "Ready",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::NoToken => "○",
            Self::InvalidToken => "✕",
            Self::ConnectivityError => "~",
            Self::AccessOrConfigIncomplete => "!",
            Self::Ready => "●",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GitHubHealthDetails {
    pub identity: Option<String>,
    pub scopes: Vec<String>,
    pub has_scope_header: bool,
    pub rate_limit_remaining: Option<u32>,
    pub rate_limit_limit: Option<u32>,
    pub configured_repo_checks: usize,
    pub accessible_repos: usize,
    pub inaccessible_repos: Vec<String>,
    pub configured_org_checks: usize,
    pub accessible_orgs: usize,
    pub inaccessible_orgs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RailwayHealthDetails {
    pub identity: Option<String>,
    pub configured_workspace_checks: usize,
    pub accessible_workspaces: usize,
    pub inaccessible_workspaces: Vec<String>,
    pub configured_project_checks: usize,
    pub accessible_projects: usize,
    pub inaccessible_projects: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VercelHealthDetails {
    pub identity: Option<String>,
    pub configured_team_checks: usize,
    pub accessible_teams: usize,
    pub inaccessible_teams: Vec<String>,
    pub configured_project_checks: usize,
    pub accessible_projects: usize,
    pub inaccessible_projects: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum PlatformHealthDetails {
    GitHub(GitHubHealthDetails),
    Railway(RailwayHealthDetails),
    Vercel(VercelHealthDetails),
    None,
}

#[derive(Debug, Clone)]
pub struct PlatformHealthReport {
    pub platform: PlatformKind,
    pub state: PlatformHealthState,
    pub reason: String,
    pub next_action: String,
    pub token_source: Option<String>,
    pub last_checked_at: DateTime<Utc>,
    pub details: PlatformHealthDetails,
}

impl PlatformHealthReport {
    pub fn is_ready(&self) -> bool {
        self.state == PlatformHealthState::Ready
    }
}

pub async fn check_all_platforms_health(
    config: &PulsosConfig,
    resolver: &TokenResolver,
    cache: &Arc<CacheStore>,
) -> Vec<PlatformHealthReport> {
    let mut reports = Vec::with_capacity(PlatformKind::ALL.len());
    for platform in PlatformKind::ALL {
        reports.push(check_platform_health(platform, config, resolver, cache).await);
    }
    reports
}

pub async fn check_platform_health(
    platform: PlatformKind,
    config: &PulsosConfig,
    resolver: &TokenResolver,
    cache: &Arc<CacheStore>,
) -> PlatformHealthReport {
    let now = Utc::now();
    let Some((token, source)) = resolver.resolve_with_source(&platform) else {
        return PlatformHealthReport {
            platform,
            state: PlatformHealthState::NoToken,
            reason: "No token was detected in env, keyring, or CLI config.".to_string(),
            next_action: format!("Run `pulsos auth {}`.", platform.cli_name()),
            token_source: None,
            last_checked_at: now,
            details: PlatformHealthDetails::None,
        };
    };
    let token_source = Some(source.to_string());

    match platform {
        PlatformKind::GitHub => check_github(config, token, cache, now, token_source).await,
        PlatformKind::Railway => check_railway(config, token, cache, now, token_source).await,
        PlatformKind::Vercel => check_vercel(config, token, cache, now, token_source).await,
    }
}

fn classify_error(err: &PulsosError) -> PlatformHealthState {
    match err {
        PulsosError::AuthFailed { .. }
        | PulsosError::TokenExpired { .. }
        | PulsosError::InsufficientScopes { .. } => PlatformHealthState::InvalidToken,

        PulsosError::GraphqlError { message, .. }
            if message.to_ascii_lowercase().contains("not authorized") =>
        {
            PlatformHealthState::InvalidToken
        }

        PulsosError::GraphqlError { .. }
        | PulsosError::Network { .. }
        | PulsosError::RateLimited { .. } => PlatformHealthState::ConnectivityError,

        PulsosError::ApiError { status, .. } => {
            if *status == 401 || *status == 403 {
                PlatformHealthState::InvalidToken
            } else if (500..=599).contains(status) {
                PlatformHealthState::ConnectivityError
            } else {
                PlatformHealthState::AccessOrConfigIncomplete
            }
        }

        PulsosError::ParseError { .. }
        | PulsosError::Config(_)
        | PulsosError::Cache(_)
        | PulsosError::Keyring(_)
        | PulsosError::NoConfig => PlatformHealthState::AccessOrConfigIncomplete,

        PulsosError::Other(_) => PlatformHealthState::ConnectivityError,
    }
}

fn error_to_action(platform: PlatformKind, state: PlatformHealthState) -> String {
    match state {
        PlatformHealthState::NoToken | PlatformHealthState::InvalidToken => {
            format!("Run `pulsos auth {}`.", platform.cli_name())
        }
        PlatformHealthState::ConnectivityError => {
            "Check network/provider status and run `pulsos doctor`.".to_string()
        }
        PlatformHealthState::AccessOrConfigIncomplete => {
            "Run `pulsos config wizard` or `pulsos repos sync` to repair setup.".to_string()
        }
        PlatformHealthState::Ready => "No action needed.".to_string(),
    }
}

async fn check_github(
    config: &PulsosConfig,
    token: secrecy::SecretString,
    cache: &Arc<CacheStore>,
    now: DateTime<Utc>,
    token_source: Option<String>,
) -> PlatformHealthReport {
    let client = GitHubClient::new(token, cache.clone());

    let auth = match client.validate_auth().await {
        Ok(status) => status,
        Err(err) => {
            let state = classify_error(&err);
            return PlatformHealthReport {
                platform: PlatformKind::GitHub,
                state,
                reason: err.user_message(),
                next_action: error_to_action(PlatformKind::GitHub, state),
                token_source,
                last_checked_at: now,
                details: PlatformHealthDetails::GitHub(GitHubHealthDetails::default()),
            };
        }
    };

    let mut details = GitHubHealthDetails {
        identity: Some(auth.identity),
        has_scope_header: !auth.scopes.is_empty(),
        scopes: auth.scopes,
        ..Default::default()
    };

    if let Ok(rate) = client.rate_limit_status().await {
        details.rate_limit_limit = Some(rate.limit);
        details.rate_limit_remaining = Some(rate.remaining);
    }

    let mut inaccessible_repos = Vec::new();
    let mut accessible_repos = 0usize;

    let configured_repos: HashSet<String> = config
        .correlations
        .iter()
        .filter_map(|c| c.github_repo.clone())
        .collect();

    details.configured_repo_checks = configured_repos.len();

    if !configured_repos.is_empty() {
        match client.fetch_user_login().await {
            Ok(login) => {
                for repo in &configured_repos {
                    let parts: Vec<&str> = repo.splitn(2, '/').collect();
                    if parts.len() != 2 {
                        inaccessible_repos.push(repo.clone());
                        continue;
                    }

                    match client
                        .check_repo_permission(parts[0], parts[1], &login)
                        .await
                    {
                        Ok(permission) if permission != "none" => {
                            accessible_repos += 1;
                        }
                        Ok(_) => inaccessible_repos.push(repo.clone()),
                        Err(_) => inaccessible_repos.push(repo.clone()),
                    }
                }
            }
            Err(err) => {
                let state = classify_error(&err);
                return PlatformHealthReport {
                    platform: PlatformKind::GitHub,
                    state,
                    reason: err.user_message(),
                    next_action: error_to_action(PlatformKind::GitHub, state),
                    token_source: token_source.clone(),
                    last_checked_at: now,
                    details: PlatformHealthDetails::GitHub(details),
                };
            }
        }
    }

    details.accessible_repos = accessible_repos;
    details.inaccessible_repos = inaccessible_repos;

    let configured_orgs: HashSet<String> = config
        .github
        .organizations
        .iter()
        .map(|o| o.name.clone())
        .collect();
    details.configured_org_checks = configured_orgs.len();

    if !configured_orgs.is_empty() {
        match client.discover().await {
            Ok(discovered) => {
                let discovered_orgs: HashSet<String> =
                    discovered.into_iter().map(|r| r.group).collect();
                for org in &configured_orgs {
                    if discovered_orgs.contains(org) {
                        details.accessible_orgs += 1;
                    } else {
                        details.inaccessible_orgs.push(org.clone());
                    }
                }
            }
            Err(err) => {
                let state = classify_error(&err);
                return PlatformHealthReport {
                    platform: PlatformKind::GitHub,
                    state,
                    reason: err.user_message(),
                    next_action: error_to_action(PlatformKind::GitHub, state),
                    token_source: token_source.clone(),
                    last_checked_at: now,
                    details: PlatformHealthDetails::GitHub(details),
                };
            }
        }
    }

    if !details.inaccessible_repos.is_empty() || !details.inaccessible_orgs.is_empty() {
        let mut reasons = Vec::new();
        if !details.inaccessible_repos.is_empty() {
            reasons.push(format!(
                "{} repo(s) are configured but not accessible",
                details.inaccessible_repos.len()
            ));
        }
        if !details.inaccessible_orgs.is_empty() {
            reasons.push(format!(
                "{} org filter(s) are not accessible",
                details.inaccessible_orgs.len()
            ));
        }

        return PlatformHealthReport {
            platform: PlatformKind::GitHub,
            state: PlatformHealthState::AccessOrConfigIncomplete,
            reason: reasons.join("; "),
            next_action: "Check repo/org access and token scopes, then run `pulsos repos verify`."
                .to_string(),
            token_source,
            last_checked_at: now,
            details: PlatformHealthDetails::GitHub(details),
        };
    }

    let reason = if details.configured_repo_checks == 0 && details.configured_org_checks == 0 {
        "Authenticated. No GitHub-specific resource filters configured.".to_string()
    } else {
        "Authenticated and configured GitHub resources are accessible.".to_string()
    };

    PlatformHealthReport {
        platform: PlatformKind::GitHub,
        state: PlatformHealthState::Ready,
        reason,
        next_action: "No action needed.".to_string(),
        token_source,
        last_checked_at: now,
        details: PlatformHealthDetails::GitHub(details),
    }
}

async fn check_railway(
    config: &PulsosConfig,
    token: secrecy::SecretString,
    cache: &Arc<CacheStore>,
    now: DateTime<Utc>,
    token_source: Option<String>,
) -> PlatformHealthReport {
    let client = RailwayClient::new(token, cache.clone());

    let auth = match client.validate_auth().await {
        Ok(status) => status,
        Err(err) => {
            let state = classify_error(&err);
            return PlatformHealthReport {
                platform: PlatformKind::Railway,
                state,
                reason: err.user_message(),
                next_action: error_to_action(PlatformKind::Railway, state),
                token_source,
                last_checked_at: now,
                details: PlatformHealthDetails::Railway(RailwayHealthDetails::default()),
            };
        }
    };

    let mut details = RailwayHealthDetails {
        identity: Some(auth.identity),
        ..Default::default()
    };

    let configured_workspaces: HashSet<String> = config
        .railway
        .workspaces
        .iter()
        .map(|w| w.name.clone())
        .collect();
    let configured_project_ids: HashSet<String> = config
        .correlations
        .iter()
        .filter_map(|c| c.railway_project.clone())
        .map(|project| {
            if let Some((project_id, _)) = project.split_once(':') {
                project_id.to_string()
            } else {
                project
            }
        })
        .collect();

    details.configured_workspace_checks = configured_workspaces.len();
    details.configured_project_checks = configured_project_ids.len();

    match client.discover().await {
        Ok(resources) => {
            let discovered_workspaces: HashSet<String> =
                resources.iter().map(|r| r.group.clone()).collect();
            let discovered_project_ids: HashSet<String> = resources
                .iter()
                .filter_map(|r| r.platform_id.split(':').next().map(ToString::to_string))
                .collect();

            for workspace in &configured_workspaces {
                if discovered_workspaces.contains(workspace) {
                    details.accessible_workspaces += 1;
                } else {
                    details.inaccessible_workspaces.push(workspace.clone());
                }
            }

            for project in &configured_project_ids {
                if discovered_project_ids.contains(project) {
                    details.accessible_projects += 1;
                } else {
                    details.inaccessible_projects.push(project.clone());
                }
            }
        }
        Err(err) => {
            let state = classify_error(&err);
            return PlatformHealthReport {
                platform: PlatformKind::Railway,
                state,
                reason: err.user_message(),
                next_action: error_to_action(PlatformKind::Railway, state),
                token_source: token_source.clone(),
                last_checked_at: now,
                details: PlatformHealthDetails::Railway(details),
            };
        }
    }

    if !details.inaccessible_workspaces.is_empty() || !details.inaccessible_projects.is_empty() {
        let mut reasons = Vec::new();
        if !details.inaccessible_workspaces.is_empty() {
            reasons.push(format!(
                "{} workspace filter(s) are not accessible",
                details.inaccessible_workspaces.len()
            ));
        }
        if !details.inaccessible_projects.is_empty() {
            reasons.push(format!(
                "{} tracked project(s) are not accessible",
                details.inaccessible_projects.len()
            ));
        }

        return PlatformHealthReport {
            platform: PlatformKind::Railway,
            state: PlatformHealthState::AccessOrConfigIncomplete,
            reason: reasons.join("; "),
            next_action:
                "Use an Account token and rerun `pulsos repos sync` to refresh project mappings."
                    .to_string(),
            token_source,
            last_checked_at: now,
            details: PlatformHealthDetails::Railway(details),
        };
    }

    let reason =
        if details.configured_workspace_checks == 0 && details.configured_project_checks == 0 {
            "Authenticated. No Railway-specific resource filters configured.".to_string()
        } else {
            "Authenticated and configured Railway resources are accessible.".to_string()
        };

    PlatformHealthReport {
        platform: PlatformKind::Railway,
        state: PlatformHealthState::Ready,
        reason,
        next_action: "No action needed.".to_string(),
        token_source,
        last_checked_at: now,
        details: PlatformHealthDetails::Railway(details),
    }
}

async fn check_vercel(
    config: &PulsosConfig,
    token: secrecy::SecretString,
    cache: &Arc<CacheStore>,
    now: DateTime<Utc>,
    token_source: Option<String>,
) -> PlatformHealthReport {
    let client = VercelClient::new(token, cache.clone());

    let auth = match client.validate_auth().await {
        Ok(status) => status,
        Err(err) => {
            let state = classify_error(&err);
            return PlatformHealthReport {
                platform: PlatformKind::Vercel,
                state,
                reason: err.user_message(),
                next_action: error_to_action(PlatformKind::Vercel, state),
                token_source,
                last_checked_at: now,
                details: PlatformHealthDetails::Vercel(VercelHealthDetails::default()),
            };
        }
    };

    let mut details = VercelHealthDetails {
        identity: Some(auth.identity),
        ..Default::default()
    };

    let configured_teams: HashSet<String> =
        config.vercel.teams.iter().map(|t| t.name.clone()).collect();
    let configured_projects: HashSet<String> = config
        .correlations
        .iter()
        .filter_map(|c| c.vercel_project.clone())
        .collect();

    details.configured_team_checks = configured_teams.len();
    details.configured_project_checks = configured_projects.len();

    match client.discover().await {
        Ok(resources) => {
            let discovered_teams: HashSet<String> =
                resources.iter().map(|r| r.group.clone()).collect();
            let discovered_projects: HashSet<String> =
                resources.iter().map(|r| r.platform_id.clone()).collect();

            for team in &configured_teams {
                if discovered_teams.contains(team) {
                    details.accessible_teams += 1;
                } else {
                    details.inaccessible_teams.push(team.clone());
                }
            }

            for project in &configured_projects {
                if discovered_projects.contains(project) {
                    details.accessible_projects += 1;
                } else {
                    details.inaccessible_projects.push(project.clone());
                }
            }
        }
        Err(err) => {
            let state = classify_error(&err);
            return PlatformHealthReport {
                platform: PlatformKind::Vercel,
                state,
                reason: err.user_message(),
                next_action: error_to_action(PlatformKind::Vercel, state),
                token_source: token_source.clone(),
                last_checked_at: now,
                details: PlatformHealthDetails::Vercel(details),
            };
        }
    }

    if !details.inaccessible_teams.is_empty() || !details.inaccessible_projects.is_empty() {
        let mut reasons = Vec::new();
        if !details.inaccessible_teams.is_empty() {
            reasons.push(format!(
                "{} team filter(s) are not accessible",
                details.inaccessible_teams.len()
            ));
        }
        if !details.inaccessible_projects.is_empty() {
            reasons.push(format!(
                "{} tracked project(s) are not accessible",
                details.inaccessible_projects.len()
            ));
        }

        return PlatformHealthReport {
            platform: PlatformKind::Vercel,
            state: PlatformHealthState::AccessOrConfigIncomplete,
            reason: reasons.join("; "),
            next_action:
                "Refresh token scope and rerun `pulsos repos sync` to update project mappings."
                    .to_string(),
            token_source,
            last_checked_at: now,
            details: PlatformHealthDetails::Vercel(details),
        };
    }

    let reason = if details.configured_team_checks == 0 && details.configured_project_checks == 0 {
        "Authenticated. No Vercel-specific resource filters configured.".to_string()
    } else {
        "Authenticated and configured Vercel resources are accessible.".to_string()
    };

    PlatformHealthReport {
        platform: PlatformKind::Vercel,
        state: PlatformHealthState::Ready,
        reason,
        next_action: "No action needed.".to_string(),
        token_source,
        last_checked_at: now,
        details: PlatformHealthDetails::Vercel(details),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_auth_errors() {
        let auth_err = PulsosError::AuthFailed {
            platform: "GitHub".to_string(),
            reason: "bad token".to_string(),
        };
        assert_eq!(classify_error(&auth_err), PlatformHealthState::InvalidToken);

        let net_err = PulsosError::Network {
            platform: "GitHub".to_string(),
            message: "timeout".to_string(),
            source: None,
        };
        assert_eq!(
            classify_error(&net_err),
            PlatformHealthState::ConnectivityError
        );

        let api_err = PulsosError::ApiError {
            platform: "GitHub".to_string(),
            status: 403,
            body: "forbidden".to_string(),
        };
        assert_eq!(classify_error(&api_err), PlatformHealthState::InvalidToken);
    }

    #[test]
    fn state_labels_and_icons() {
        assert_eq!(PlatformHealthState::NoToken.label(), "No Token");
        assert_eq!(PlatformHealthState::NoToken.icon(), "○");
        assert_eq!(PlatformHealthState::Ready.label(), "Ready");
        assert_eq!(PlatformHealthState::Ready.icon(), "●");
    }
}
