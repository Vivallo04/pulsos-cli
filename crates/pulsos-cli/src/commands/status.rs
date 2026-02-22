use crate::commands::ui::screen::{screen_confirm, PromptResult, ScreenSession, ScreenSpec};
use crate::output::{self, OutputFormat};
use anyhow::Result;
use clap::Args;
use pulsos_core::auth::credential_store::{CredentialStore, FallbackStore};
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::load_config;
use pulsos_core::correlation;
use pulsos_core::domain::deployment::DeploymentEvent;
use pulsos_core::domain::health;
use pulsos_core::error::PulsosError;
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::{PlatformAdapter, TrackedResource};
use std::io::IsTerminal;
use std::path::Path;
use std::sync::Arc;

use super::wizard::{needs_wizard_prompt, run_config_wizard};

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

    /// Force one-shot output (disable auto-live mode)
    #[arg(long, conflicts_with = "watch")]
    pub once: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunMode {
    Live,
    Once,
}

fn resolve_run_mode(
    args: &StatusArgs,
    format: OutputFormat,
    stdout_is_tty: bool,
) -> Result<RunMode> {
    if args.watch {
        if !stdout_is_tty {
            anyhow::bail!("`--watch` requires an interactive terminal (TTY).");
        }
        return Ok(RunMode::Live);
    }

    if args.once {
        return Ok(RunMode::Once);
    }

    if !stdout_is_tty {
        return Ok(RunMode::Once);
    }

    if !matches!(format, OutputFormat::Table) {
        return Ok(RunMode::Once);
    }

    Ok(RunMode::Live)
}

pub async fn execute(
    args: StatusArgs,
    format: OutputFormat,
    _no_color: bool,
    config_path: Option<&Path>,
    log_buffer: crate::tui::log_buffer::LogRingBuffer,
    tui_active: crate::tui::log_buffer::TuiActiveFlag,
) -> Result<()> {
    let stdout_is_tty = std::io::stdout().is_terminal();

    let mut config = match load_config(config_path) {
        Ok(c) => c,
        Err(PulsosError::NoConfig) => {
            if stdout_is_tty {
                run_config_wizard(config_path).await?;
                load_config(config_path).map_err(|_| {
                    anyhow::anyhow!(
                        "Wizard completed but no config was saved. Run `pulsos repos sync` manually."
                    )
                })?
            } else {
                anyhow::bail!(
                    "No configuration found. Run `pulsos repos sync` to discover and track your projects."
                );
            }
        }
        Err(other) => {
            anyhow::bail!("{}", other.user_message());
        }
    };

    if stdout_is_tty && needs_wizard_prompt(&config).await.unwrap_or(false) {
        let session = ScreenSession::new();
        let spec = ScreenSpec::new("Setup Check").body_lines([
            "Some platform checks are incomplete.",
            "Run setup wizard now to validate and fix configuration?",
        ]);
        let should_run = match screen_confirm(&session, &spec, "Run setup wizard now?", true) {
            Ok(PromptResult {
                cancelled: true, ..
            }) => false,
            Ok(PromptResult {
                value: Some(value), ..
            }) => value,
            _ => false,
        };

        if should_run {
            run_config_wizard(config_path).await?;
            config = load_config(config_path).map_err(|_| {
                anyhow::anyhow!(
                    "Wizard completed but no config was saved. Run `pulsos repos sync` manually."
                )
            })?;
        }
    }

    let run_mode = resolve_run_mode(&args, format, stdout_is_tty)?;
    if run_mode == RunMode::Live {
        return crate::tui::run_tui(
            config,
            config_path.map(|p| p.to_path_buf()),
            log_buffer,
            tui_active,
        )
        .await;
    }

    let cache = Arc::new(CacheStore::open_or_temporary()?);
    let store: Arc<dyn CredentialStore> = Arc::new(FallbackStore::new()?);
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
            let client = GitHubClient::new(token, cache.clone())?;
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
            let client = RailwayClient::new(token, cache.clone())?;
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
            let client = VercelClient::new(token, cache.clone())?;
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
                .map_or(true, |b| b.contains(branch.as_str()))
        });
    }

    // Sort by created_at descending
    all_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let correlated = correlation::correlate_all(&config.correlations, &all_events);
    let health_scores = health::compute_project_health_scores(&config.correlations, &all_events);

    match format {
        OutputFormat::Table => {
            output::table::render_correlated(&correlated);
            output::table::render_health_scores(&health_scores);
        }
        OutputFormat::Json => {
            output::json::render_correlated_with_health(&correlated, &health_scores)?
        }
        OutputFormat::Compact => output::compact::render_correlated(&correlated),
    }

    if !warnings.is_empty() {
        eprintln!();
        for w in &warnings {
            eprintln!("  warning: {w}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> StatusArgs {
        StatusArgs {
            project: None,
            platform: None,
            view: None,
            branch: None,
            watch: false,
            once: false,
        }
    }

    #[test]
    fn tty_table_defaults_to_live() {
        let args = base_args();
        let mode = resolve_run_mode(&args, OutputFormat::Table, true).unwrap();
        assert_eq!(mode, RunMode::Live);
    }

    #[test]
    fn tty_table_once_forces_one_shot() {
        let mut args = base_args();
        args.once = true;
        let mode = resolve_run_mode(&args, OutputFormat::Table, true).unwrap();
        assert_eq!(mode, RunMode::Once);
    }

    #[test]
    fn watch_forces_live_on_tty() {
        let mut args = base_args();
        args.watch = true;
        let mode = resolve_run_mode(&args, OutputFormat::Table, true).unwrap();
        assert_eq!(mode, RunMode::Live);
    }

    #[test]
    fn non_tty_defaults_to_one_shot() {
        let args = base_args();
        let mode = resolve_run_mode(&args, OutputFormat::Table, false).unwrap();
        assert_eq!(mode, RunMode::Once);
    }

    #[test]
    fn non_table_formats_are_one_shot() {
        let args = base_args();
        let json_mode = resolve_run_mode(&args, OutputFormat::Json, true).unwrap();
        let compact_mode = resolve_run_mode(&args, OutputFormat::Compact, true).unwrap();
        assert_eq!(json_mode, RunMode::Once);
        assert_eq!(compact_mode, RunMode::Once);
    }

    #[test]
    fn watch_without_tty_errors() {
        let mut args = base_args();
        args.watch = true;
        let err = resolve_run_mode(&args, OutputFormat::Table, false)
            .expect_err("watch without tty should fail");
        assert!(err.to_string().contains("interactive terminal"));
    }
}
