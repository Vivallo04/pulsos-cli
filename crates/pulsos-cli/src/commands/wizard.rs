use std::io::IsTerminal;
use std::path::Path;
use std::sync::Arc;

use crate::commands::ui::screen::{
    screen_confirm, screen_multiselect, PromptResult, ScreenSession, ScreenSeverity, ScreenSpec,
};
use anyhow::Result;
use pulsos_core::auth::credential_store::{CredentialStore, FallbackStore};
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::load_config;
use pulsos_core::config::types::PulsosConfig;
use pulsos_core::health::{check_all_platforms_health, PlatformHealthReport, PlatformHealthState};

use super::auth::auth_platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WizardStep {
    SelectPlatforms,
    Authenticate,
    DiscoverAndTrack,
    Recheck,
}

#[derive(Debug, Default)]
struct WizardProgress {
    current: Option<WizardStep>,
    completed: Vec<WizardStep>,
}

impl WizardProgress {
    fn start(&mut self, step: WizardStep) {
        self.current = Some(step);
    }

    fn finish(&mut self) {
        if let Some(step) = self.current.take() {
            if !self.completed.contains(&step) {
                self.completed.push(step);
            }
        }
    }
}

pub async fn run_config_wizard(config_path: Option<&Path>) -> Result<()> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("`pulsos config wizard` requires an interactive terminal (TTY).")
    }

    let screen = ScreenSession::new();
    screen.render(
        &ScreenSpec::new("P U L S O S")
            .subtitle("No configuration found — let's set up Pulsos.")
            .body_lines([
                "This wizard checks tokens, discovers projects, and saves config.",
                "Navigation is linear: continue or cancel.",
            ]),
    )?;
    let mut progress = WizardProgress::default();

    let existing_config = load_config(config_path).unwrap_or_default();
    let cache = Arc::new(CacheStore::open_or_temporary()?);
    let store: Arc<dyn CredentialStore> = Arc::new(FallbackStore::new()?);
    let resolver = TokenResolver::new(store.clone(), existing_config.auth.token_detection.clone());

    let reports = check_all_platforms_health(&existing_config, &resolver, &cache).await;
    let defaults: Vec<bool> = vec![false; PlatformKind::ALL.len()];

    let items: Vec<String> = PlatformKind::ALL
        .iter()
        .map(|platform| {
            let state = reports
                .iter()
                .find(|r| r.platform == *platform)
                .map(|r| r.state)
                .unwrap_or(PlatformHealthState::NoToken);
            format!(
                "{} [{} {}]",
                platform.display_name(),
                state.icon(),
                state.label()
            )
        })
        .collect();

    progress.start(WizardStep::SelectPlatforms);
    let selection_spec = ScreenSpec::new("Platform Selection")
        .step(1, 4)
        .body_lines([
            "Select platforms to configure.",
            "All options are disabled by default.",
        ])
        .hints(["Use arrows + space to toggle, Enter to continue."]);
    let selected = match screen_multiselect(
        &screen,
        &selection_spec,
        "Select platforms to configure",
        &items,
        &defaults,
    )? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => vec![],
    };
    progress.finish();

    progress.start(WizardStep::Authenticate);
    let selected_platforms: Vec<PlatformKind> = selected
        .into_iter()
        .map(|idx| PlatformKind::ALL[idx])
        .collect();

    let total = selected_platforms.len();
    for (i, platform) in selected_platforms.iter().enumerate() {
        let step = (i + 1, total.max(1));
        loop {
            match auth_platform(
                *platform,
                &store,
                &cache,
                false,
                None,
                Some(&screen),
                Some(step),
            )
            .await
            {
                Ok(()) => break,
                Err(e) => {
                    let retry_spec = ScreenSpec::new(platform.display_name())
                        .step(step.0, step.1)
                        .subtitle("Authentication")
                        .body_lines([format!("Auth failed: {e}")])
                        .hints(["Retry this platform?".to_string()])
                        .severity(ScreenSeverity::Error);
                    let retry = match screen_confirm(&screen, &retry_spec, "Retry?", true)? {
                        PromptResult {
                            cancelled: true, ..
                        } => false,
                        PromptResult {
                            value: Some(value), ..
                        } => value,
                        _ => false,
                    };
                    if !retry {
                        break;
                    }
                }
            }
        }
    }
    progress.finish();

    let any_platform_token = has_any_platform_token(&store)?;
    let mut discovery_skipped_no_tokens = false;
    let mut run_sync = false;

    if any_platform_token {
        progress.start(WizardStep::DiscoverAndTrack);
        let discover_spec = ScreenSpec::new("Discovery")
            .step(3, 4)
            .body_lines(["Discover and track projects across configured platforms now?"]);
        run_sync = match screen_confirm(
            &screen,
            &discover_spec,
            "Discover and track projects now?",
            true,
        )? {
            PromptResult {
                cancelled: true, ..
            } => false,
            PromptResult {
                value: Some(value), ..
            } => value,
            _ => false,
        };
        progress.finish();
    } else {
        discovery_skipped_no_tokens = true;
    }

    if run_sync {
        let sync_args = super::repos::ReposArgs { command: None };
        super::repos::execute_with_store(sync_args, config_path, Some(store.clone())).await?;
    }

    progress.start(WizardStep::Recheck);
    let final_config = load_config(config_path).unwrap_or(existing_config);
    // Re-open credential store to avoid session-only false positives.
    let final_store: Arc<dyn CredentialStore> = Arc::new(FallbackStore::new()?);
    let final_resolver = TokenResolver::new(final_store, final_config.auth.token_detection.clone());
    let final_reports = check_all_platforms_health(&final_config, &final_resolver, &cache).await;
    render_health_summary(&screen, &final_reports, discovery_skipped_no_tokens)?;
    progress.finish();

    Ok(())
}

pub async fn needs_wizard_prompt(config: &PulsosConfig) -> Result<bool> {
    let cache = Arc::new(CacheStore::open_or_temporary()?);
    let store: Arc<dyn CredentialStore> = Arc::new(FallbackStore::new()?);
    let resolver = TokenResolver::new(store, config.auth.token_detection.clone());
    let reports = check_all_platforms_health(config, &resolver, &cache).await;

    let needs_setup = reports.iter().any(|report| {
        platform_is_enabled(config, report.platform)
            && matches!(
                report.state,
                PlatformHealthState::NoToken | PlatformHealthState::InvalidToken
            )
    });

    Ok(needs_setup)
}

fn platform_is_enabled(config: &PulsosConfig, platform: PlatformKind) -> bool {
    match platform {
        PlatformKind::GitHub => {
            !config.github.organizations.is_empty()
                || config
                    .correlations
                    .iter()
                    .any(|corr| corr.github_repo.is_some())
        }
        PlatformKind::Railway => {
            !config.railway.workspaces.is_empty()
                || config
                    .correlations
                    .iter()
                    .any(|corr| corr.railway_project.is_some())
        }
        PlatformKind::Vercel => {
            !config.vercel.teams.is_empty()
                || config
                    .correlations
                    .iter()
                    .any(|corr| corr.vercel_project.is_some())
        }
    }
}

fn has_any_platform_token(store: &Arc<dyn CredentialStore>) -> Result<bool> {
    for platform in PlatformKind::ALL {
        if store.get(&platform)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn render_health_summary(
    screen: &ScreenSession,
    reports: &[PlatformHealthReport],
    discovery_skipped_no_tokens: bool,
) -> Result<()> {
    let mut lines = vec!["Platform readiness summary".to_string(), "─".repeat(50)];
    if discovery_skipped_no_tokens {
        lines.push("Discovery skipped: no platform tokens are configured.".to_string());
        lines.push(String::new());
    }
    for report in reports {
        lines.push(format!(
            "{} {:<8} {:<16} {}",
            report.state.icon(),
            report.platform.display_name(),
            report.state.label(),
            report.reason
        ));
    }
    let spec = ScreenSpec::new("Summary")
        .step(4, 4)
        .body_lines(lines)
        .hints(["Run `pulsos status` to start live monitoring."]);
    screen.render(&spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsos_core::auth::credential_store::{CredentialStore, InMemoryStore};
    use std::sync::Arc;

    fn base_config() -> PulsosConfig {
        PulsosConfig::default()
    }

    #[test]
    fn enabled_platform_detection() {
        let mut cfg = base_config();
        assert!(!platform_is_enabled(&cfg, PlatformKind::GitHub));

        cfg.github
            .organizations
            .push(pulsos_core::config::types::OrgConfig {
                name: "my-org".to_string(),
                include_patterns: vec![],
                exclude_patterns: vec![],
                auto_discover: true,
            });

        assert!(platform_is_enabled(&cfg, PlatformKind::GitHub));
    }

    #[test]
    fn token_presence_detection() {
        let store: Arc<dyn CredentialStore> = Arc::new(InMemoryStore::new());
        assert!(!has_any_platform_token(&store).unwrap());

        store
            .set(&PlatformKind::GitHub, "token-value")
            .expect("set token");
        assert!(has_any_platform_token(&store).unwrap());
    }
}
