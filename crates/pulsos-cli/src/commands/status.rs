use crate::output::{self, OutputFormat};
use anyhow::Result;
use clap::Args;
use pulsos_core::auth::credential_store::KeyringStore;
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::load_config;
use pulsos_core::domain::deployment::DeploymentEvent;
use pulsos_core::error::PulsosError;
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::{PlatformAdapter, TrackedResource};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Filter to a specific project
    pub project: Option<String>,

    /// Filter to one platform
    #[arg(long, value_parser = ["github", "railway", "vercel"])]
    pub platform: Option<String>,

    /// Use a saved view
    #[arg(long)]
    pub view: Option<String>,

    /// Filter by branch pattern
    #[arg(long)]
    pub branch: Option<String>,

    /// Live-updating TUI mode
    #[arg(long)]
    pub watch: bool,
}

pub async fn execute(
    args: StatusArgs,
    format: OutputFormat,
    _no_color: bool,
    config_path: Option<&Path>,
) -> Result<()> {
    let config = load_config(config_path).map_err(|e| match e {
        PulsosError::NoConfig => {
            anyhow::anyhow!(
                "No configuration found. Run `pulsos repos sync` to discover and track your projects."
            )
        }
        other => anyhow::anyhow!("{}", other.user_message()),
    })?;

    let cache = Arc::new(CacheStore::open_default()?);
    let store = Arc::new(KeyringStore::new());
    let resolver = TokenResolver::new(store, config.auth.token_detection.clone());

    let mut all_events: Vec<DeploymentEvent> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let selected_view = if let Some(view_name) = args.view.as_ref() {
        Some(
            config
                .views
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(view_name))
                .ok_or_else(|| anyhow::anyhow!("Saved view not found: {view_name}"))?,
        )
    } else {
        None
    };

    let branch_filter = if args.branch.is_some() {
        args.branch.clone()
    } else if let Some(view) = selected_view {
        view.branch_filter.clone()
    } else {
        None
    };

    let should_fetch = |platform: &str| -> bool {
        if let Some(p) = args.platform.as_ref() {
            return p.eq_ignore_ascii_case(platform);
        }
        if let Some(view) = selected_view {
            if !view.platforms.is_empty() {
                return view
                    .platforms
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case(platform));
            }
        }
        true
    };

    let correlation_in_scope = |name: &str| -> bool {
        if let Some(project_filter) = args.project.as_ref() {
            let needle = project_filter.to_ascii_lowercase();
            if !name.to_ascii_lowercase().contains(&needle) {
                return false;
            }
        }

        if let Some(view) = selected_view {
            if !view.projects.is_empty() {
                return view.projects.iter().any(|p| p.eq_ignore_ascii_case(name));
            }
        }

        true
    };

    // Build tracked resources from correlations config.
    // Each correlation binds a github_repo, railway_project, and vercel_project.
    let mut github_tracked: Vec<TrackedResource> = Vec::new();
    let mut railway_tracked: Vec<TrackedResource> = Vec::new();
    let mut vercel_tracked: Vec<TrackedResource> = Vec::new();

    for corr in &config.correlations {
        if !correlation_in_scope(&corr.name) {
            continue;
        }

        if let Some(ref repo) = corr.github_repo {
            github_tracked.push(TrackedResource {
                platform_id: repo.clone(),
                display_name: repo.split('/').next_back().unwrap_or(repo).to_string(),
                group: None,
            });
        }
        if let Some(ref project) = corr.railway_project {
            // Railway platform_id is "projectId:serviceId:environmentId"
            // For now, treat the correlation value as a composite key
            railway_tracked.push(TrackedResource {
                platform_id: project.clone(),
                display_name: corr.name.clone(),
                group: corr.railway_workspace.clone(),
            });
        }
        if let Some(ref project) = corr.vercel_project {
            vercel_tracked.push(TrackedResource {
                platform_id: project.clone(),
                display_name: corr.name.clone(),
                group: corr.vercel_team.clone(),
            });
        }
    }

    // GitHub
    if should_fetch("github") && !github_tracked.is_empty() {
        if let Some(token) = resolver.resolve(&PlatformKind::GitHub) {
            let client = GitHubClient::new(token, cache.clone());
            match client.fetch_events(&github_tracked).await {
                Ok(events) => all_events.extend(events),
                Err(e) => warnings.push(format!("GitHub: {}", e.user_message())),
            }
        } else {
            warnings
                .push("GitHub: no token found. Run `pulsos auth github` to authenticate.".into());
        }
    }

    // Railway
    if should_fetch("railway") && !railway_tracked.is_empty() {
        if let Some(token) = resolver.resolve(&PlatformKind::Railway) {
            let client = RailwayClient::new(token, cache.clone());
            match client.fetch_events(&railway_tracked).await {
                Ok(events) => all_events.extend(events),
                Err(e) => warnings.push(format!("Railway: {}", e.user_message())),
            }
        } else {
            warnings
                .push("Railway: no token found. Run `pulsos auth railway` to authenticate.".into());
        }
    }

    // Vercel
    if should_fetch("vercel") && !vercel_tracked.is_empty() {
        if let Some(token) = resolver.resolve(&PlatformKind::Vercel) {
            let client = VercelClient::new(token, cache.clone());
            match client.fetch_events(&vercel_tracked).await {
                Ok(events) => all_events.extend(events),
                Err(e) => warnings.push(format!("Vercel: {}", e.user_message())),
            }
        } else {
            warnings
                .push("Vercel: no token found. Run `pulsos auth vercel` to authenticate.".into());
        }
    }

    // Apply branch filter
    if let Some(ref branch) = branch_filter {
        all_events.retain(|e| {
            e.branch
                .as_ref()
                .is_none_or(|b| b.contains(branch.as_str()))
        });
    }

    // Sort by created_at descending
    all_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    if args.watch {
        warnings
            .push("--watch mode is not implemented yet; showing a single snapshot instead.".into());
    }

    // Output
    match format {
        OutputFormat::Table => output::table::render(&all_events),
        OutputFormat::Json => output::json::render(&all_events)?,
        OutputFormat::Compact => output::compact::render(&all_events),
    }

    // Show warnings
    if !warnings.is_empty() {
        eprintln!();
        for w in &warnings {
            eprintln!("  warning: {w}");
        }
    }

    Ok(())
}
