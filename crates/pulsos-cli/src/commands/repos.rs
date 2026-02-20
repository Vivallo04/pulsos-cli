use anyhow::Result;
use crate::commands::ui::screen::{
    screen_confirm, screen_input, screen_multiselect, PromptResult, ScreenSession, ScreenSpec,
};
use clap::{Args, Subcommand};
use pulsos_core::auth::credential_store::{CredentialStore, FallbackStore};
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::types::{CorrelationConfig, GroupConfig};
use pulsos_core::config::{load_config, save_config};
use pulsos_core::health::{check_all_platforms_health, PlatformHealthState};
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::{DiscoveredResource, PlatformAdapter};
use pulsos_core::sync::correlate::{
    build_correlations, candidate_to_config, DiscoveryResults, MatchConfidence,
};
use pulsos_core::sync::merge::{merge_correlations, populate_platform_sections};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Args)]
pub struct ReposArgs {
    #[command(subcommand)]
    pub command: Option<ReposCommand>,
}

#[derive(Debug, Subcommand)]
pub enum GroupsCommand {
    /// List all groups (default)
    List,
    /// Create a new group
    Create {
        name: String,
        /// Resources as platform:id (supply after --)
        #[arg(last = true)]
        resources: Vec<String>,
    },
    /// Delete a group by name
    Delete { name: String },
}

#[derive(Debug, Subcommand)]
pub enum ReposCommand {
    /// Discover + select + save (all platforms, single step)
    Sync,
    /// Show currently tracked repos/projects
    List,
    /// Add a specific repo/project by name
    Add {
        /// e.g. github:myorg/repo or railway:project-id
        resource: String,
    },
    /// Remove a repo/project from tracking
    Remove {
        /// e.g. github:myorg/repo
        resource: String,
    },
    /// Manually set or edit platform correlations for a project
    Correlate {
        /// Project name to edit (e.g. my-saas)
        name: String,
    },
    /// Manage named groups of resources
    Groups {
        #[command(subcommand)]
        command: Option<GroupsCommand>,
    },
    /// Verify access permissions for all tracked resources
    Verify,
}

pub async fn execute(args: ReposArgs, config_path: Option<&Path>) -> Result<()> {
    execute_with_store(args, config_path, None).await
}

pub(crate) async fn execute_with_store(
    args: ReposArgs,
    config_path: Option<&Path>,
    store_override: Option<Arc<dyn CredentialStore>>,
) -> Result<()> {
    match args.command {
        Some(ReposCommand::Sync) | None => sync_command(config_path, store_override).await,
        Some(ReposCommand::List) => list_command(config_path),
        Some(ReposCommand::Add { resource }) => add_command(&resource, config_path),
        Some(ReposCommand::Remove { resource }) => remove_command(&resource, config_path),
        Some(ReposCommand::Correlate { name }) => correlate_command(&name, config_path),
        Some(ReposCommand::Groups { command }) => groups_command(command, config_path),
        Some(ReposCommand::Verify) => verify_command(config_path).await,
    }
}

async fn sync_command(
    config_path: Option<&Path>,
    store_override: Option<Arc<dyn CredentialStore>>,
) -> Result<()> {
    let screen = ScreenSession::new();
    println!("Discovering projects across platforms...");
    println!();

    // Load existing config or start fresh.
    let existing_config = load_config(config_path).unwrap_or_default();

    let cache = Arc::new(CacheStore::open_or_temporary());
    let store: Arc<dyn CredentialStore> = match store_override {
        Some(store) => store,
        None => Arc::new(FallbackStore::new()?),
    };
    let resolver = TokenResolver::new(store, existing_config.auth.token_detection.clone());

    let mut github_resources: Vec<DiscoveredResource> = Vec::new();
    let mut railway_resources: Vec<DiscoveredResource> = Vec::new();
    let mut vercel_resources: Vec<(DiscoveredResource, Option<String>)> = Vec::new();
    let mut skipped_platforms: Vec<&str> = Vec::new();

    // ── GitHub ──
    if let Some(token) = resolver.resolve(&PlatformKind::GitHub) {
        print!("  GitHub: scanning... ");
        let client = GitHubClient::new(token, cache.clone());
        match client.discover().await {
            Ok(resources) => {
                let active: Vec<_> = resources
                    .into_iter()
                    .filter(|r| !r.archived && !r.disabled)
                    .collect();
                println!("found {} repositories", active.len());
                github_resources = active;
            }
            Err(e) => println!("error: {}", e.user_message()),
        }
    } else {
        skipped_platforms.push("GitHub");
    }

    // ── Railway ──
    if let Some(token) = resolver.resolve(&PlatformKind::Railway) {
        print!("  Railway: scanning... ");
        let client = RailwayClient::new(token, cache.clone());
        match client.discover().await {
            Ok(resources) => {
                let active: Vec<_> = resources
                    .into_iter()
                    .filter(|r| !r.archived && !r.disabled)
                    .collect();
                println!("found {} services", active.len());
                railway_resources = active;
            }
            Err(e) => println!("error: {}", e.user_message()),
        }
    } else {
        skipped_platforms.push("Railway");
    }

    // ── Vercel ──
    if let Some(token) = resolver.resolve(&PlatformKind::Vercel) {
        print!("  Vercel: scanning... ");
        let client = VercelClient::new(token, cache.clone());
        match client.discover_with_links().await {
            Ok(resources) => {
                let active: Vec<_> = resources
                    .into_iter()
                    .filter(|(r, _)| !r.archived && !r.disabled)
                    .collect();
                println!("found {} projects", active.len());
                vercel_resources = active;
            }
            Err(e) => println!("error: {}", e.user_message()),
        }
    } else {
        skipped_platforms.push("Vercel");
    }

    if !skipped_platforms.is_empty() {
        println!();
        println!(
            "  Skipped (no token): {}. Run `pulsos auth <platform>` first.",
            skipped_platforms.join(", ")
        );
    }

    // If nothing was discovered, bail early.
    if github_resources.is_empty() && railway_resources.is_empty() && vercel_resources.is_empty() {
        println!();
        println!(
            "No resources discovered. Make sure you are authenticated with at least one platform."
        );
        return Ok(());
    }

    // ── Interactive selection ──
    println!();

    if !github_resources.is_empty() {
        let items: Vec<String> = github_resources
            .iter()
            .map(|r| format!("{} ({})", r.display_name, r.group))
            .collect();
        let defaults: Vec<bool> = vec![false; items.len()];
        let spec = ScreenSpec::new("GitHub Selection")
            .step(1, 4)
            .body_lines([
                "Select GitHub repositories to track.",
                "All repositories are disabled by default.",
            ]);
        let selections = match screen_multiselect(
            &screen,
            &spec,
            "Select GitHub repositories to track",
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
        github_resources = selections
            .into_iter()
            .map(|i| github_resources[i].clone())
            .collect();
    }

    if !railway_resources.is_empty() {
        let items: Vec<String> = railway_resources
            .iter()
            .map(|r| format!("{} ({})", r.display_name, r.group))
            .collect();
        let defaults: Vec<bool> = vec![false; items.len()];
        let spec = ScreenSpec::new("Railway Selection")
            .step(2, 4)
            .body_lines([
                "Select Railway services to track.",
                "All services are disabled by default.",
            ]);
        let selections = match screen_multiselect(
            &screen,
            &spec,
            "Select Railway services to track",
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
        railway_resources = selections
            .into_iter()
            .map(|i| railway_resources[i].clone())
            .collect();
    }

    if !vercel_resources.is_empty() {
        let items: Vec<String> = vercel_resources
            .iter()
            .map(|(r, linked)| {
                let linked_str = linked
                    .as_ref()
                    .map(|l| format!(" → {l}"))
                    .unwrap_or_default();
                format!("{} ({}){}", r.display_name, r.group, linked_str)
            })
            .collect();
        let defaults: Vec<bool> = vec![false; items.len()];
        let spec = ScreenSpec::new("Vercel Selection")
            .step(3, 4)
            .body_lines([
                "Select Vercel projects to track.",
                "All projects are disabled by default.",
            ]);
        let selections = match screen_multiselect(
            &screen,
            &spec,
            "Select Vercel projects to track",
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
        vercel_resources = selections
            .into_iter()
            .map(|i| vercel_resources[i].clone())
            .collect();
    }

    // ── Auto-correlate ──
    let discovery = DiscoveryResults {
        github: github_resources,
        railway: railway_resources,
        vercel: vercel_resources,
    };

    let candidates = build_correlations(&discovery);

    if candidates.is_empty() {
        println!("No resources selected.");
        return Ok(());
    }

    // ── Show proposed correlations ──
    println!();
    println!("Proposed correlations:");
    for (i, c) in candidates.iter().enumerate() {
        let confidence_str = match c.confidence {
            MatchConfidence::LinkedRepo => "linked",
            MatchConfidence::ExactStem => "name match",
            MatchConfidence::Unmatched => "standalone",
        };
        println!("  {}. {} ({})", i + 1, c.name, confidence_str);
        if let Some(ref gh) = c.github {
            println!("     GitHub:  {}", gh.platform_id);
        }
        if let Some(ref rw) = c.railway {
            println!("     Railway: {}", rw.display_name);
        }
        if let Some(ref vc) = c.vercel {
            println!("     Vercel:  {}", vc.display_name);
        }
    }

    println!();
    let confirm_spec = ScreenSpec::new("Correlation Review")
        .step(4, 4)
        .body_lines(["Review proposed correlations and accept to save config."]);
    let confirm = match screen_confirm(
        &screen,
        &confirm_spec,
        "Accept these correlations?",
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

    if !confirm {
        println!("Sync cancelled.");
        return Ok(());
    }

    // ── Convert and merge ──
    let new_correlations: Vec<CorrelationConfig> =
        candidates.iter().map(candidate_to_config).collect();

    let (mut final_config, added, updated) = merge_correlations(&existing_config, new_correlations);
    populate_platform_sections(&mut final_config);

    // ── Save ──
    save_config(&final_config, config_path)?;

    println!();
    println!(
        "Saved {} correlations ({} new, {} updated) to config.",
        final_config.correlations.len(),
        added,
        updated
    );
    println!("Run `pulsos status` to see your deployments.");

    Ok(())
}

fn list_command(config_path: Option<&Path>) -> Result<()> {
    let config = load_config(config_path).map_err(|_| {
        anyhow::anyhow!("No configuration found. Run `pulsos repos sync` to get started.")
    })?;

    if config.correlations.is_empty() {
        println!("No tracked resources. Run `pulsos repos sync` to get started.");
        return Ok(());
    }

    println!("Tracked Resources");
    println!("{}", "=".repeat(44));

    for corr in &config.correlations {
        println!();
        println!("  {}", corr.name);
        if let Some(ref gh) = corr.github_repo {
            println!("    GitHub:  {gh}");
        }
        if let Some(ref rw) = corr.railway_project {
            println!("    Railway: {rw}");
        }
        if let Some(ref vc) = corr.vercel_project {
            println!("    Vercel:  {vc}");
        }
    }

    println!();
    Ok(())
}

fn add_command(resource: &str, config_path: Option<&Path>) -> Result<()> {
    let (platform, id) = parse_resource(resource)?;

    let mut config = load_config(config_path).unwrap_or_default();

    // Derive a correlation name from the resource ID.
    let name = derive_name(&platform, id);

    // Find existing correlation by name or create new.
    let corr = if let Some(existing) = config
        .correlations
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(&name))
    {
        existing
    } else {
        config.correlations.push(CorrelationConfig {
            name: name.clone(),
            github_repo: None,
            railway_project: None,
            railway_workspace: None,
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: HashMap::new(),
        });
        config.correlations.last_mut().unwrap()
    };

    match platform.as_str() {
        "github" => corr.github_repo = Some(id.to_string()),
        "railway" => corr.railway_project = Some(id.to_string()),
        "vercel" => corr.vercel_project = Some(id.to_string()),
        _ => unreachable!(),
    }

    populate_platform_sections(&mut config);
    save_config(&config, config_path)?;

    println!("Added {platform}:{id} to correlation '{name}'.");
    Ok(())
}

fn remove_command(resource: &str, config_path: Option<&Path>) -> Result<()> {
    let (platform, id) = parse_resource(resource)?;

    let mut config = load_config(config_path)
        .map_err(|_| anyhow::anyhow!("No configuration found. Nothing to remove."))?;

    let mut removed = false;
    for corr in &mut config.correlations {
        let matches = match platform.as_str() {
            "github" => corr.github_repo.as_deref() == Some(id),
            "railway" => corr.railway_project.as_deref() == Some(id),
            "vercel" => corr.vercel_project.as_deref() == Some(id),
            _ => false,
        };

        if matches {
            match platform.as_str() {
                "github" => corr.github_repo = None,
                "railway" => {
                    corr.railway_project = None;
                    corr.railway_workspace = None;
                    corr.railway_environment = None;
                }
                "vercel" => {
                    corr.vercel_project = None;
                    corr.vercel_team = None;
                }
                _ => {}
            }
            removed = true;
            break;
        }
    }

    if !removed {
        anyhow::bail!("Resource {platform}:{id} not found in any correlation.");
    }

    // Remove empty correlations (no platform fields set).
    config.correlations.retain(|c| {
        c.github_repo.is_some() || c.railway_project.is_some() || c.vercel_project.is_some()
    });

    save_config(&config, config_path)?;
    println!("Removed {platform}:{id} from tracking.");
    Ok(())
}

fn correlate_command(name: &str, config_path: Option<&Path>) -> Result<()> {
    let screen = ScreenSession::new();
    let mut config = load_config(config_path).map_err(|_| {
        anyhow::anyhow!("No configuration found. Run `pulsos repos sync` to get started.")
    })?;

    // Find the correlation by name, or offer to create it.
    let pos = config
        .correlations
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name));

    let corr = if let Some(i) = pos {
        config.correlations[i].clone()
    } else {
        let create_spec = ScreenSpec::new("Correlation Setup")
            .body_lines([format!("Project '{name}' not found.")]);
        let create = match screen_confirm(
            &screen,
            &create_spec,
            &format!("Create project '{name}'?"),
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
        if !create {
            println!("Cancelled.");
            return Ok(());
        }
        CorrelationConfig {
            name: name.to_string(),
            github_repo: None,
            railway_project: None,
            railway_workspace: None,
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: HashMap::new(),
        }
    };

    println!("Editing correlations for '{}'", corr.name);
    println!("Press Enter to keep existing value. Type '-' to clear a field.");
    println!();

    // ── GitHub ──
    let gh_current = corr.github_repo.clone().unwrap_or_else(|| "not set".into());
    let gh_spec = ScreenSpec::new("Correlation Edit")
        .step(1, 3)
        .body_lines([format!("GitHub repo (current: {gh_current})")]);
    let gh_input: String = match screen_input(
        &screen,
        &gh_spec,
        "New value (e.g. myorg/my-saas)",
        corr.github_repo.as_deref(),
        true,
    )? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => String::new(),
    };

    // ── Railway ──
    let rw_current = corr
        .railway_project
        .clone()
        .unwrap_or_else(|| "not set".into());
    let rw_spec = ScreenSpec::new("Correlation Edit")
        .step(2, 3)
        .body_lines([format!("Railway project (current: {rw_current})")]);
    let rw_input: String = match screen_input(
        &screen,
        &rw_spec,
        "New value (project ID or name)",
        corr.railway_project.as_deref(),
        true,
    )? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => String::new(),
    };

    let rw_ws_current = corr
        .railway_workspace
        .clone()
        .unwrap_or_else(|| "not set".into());
    let rw_ws_spec = ScreenSpec::new("Correlation Edit")
        .step(2, 3)
        .body_lines([format!("Railway workspace (current: {rw_ws_current})")]);
    let rw_ws_input: String = match screen_input(
        &screen,
        &rw_ws_spec,
        "New value (workspace name)",
        corr.railway_workspace.as_deref(),
        true,
    )? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => String::new(),
    };

    // ── Vercel ──
    let vc_current = corr
        .vercel_project
        .clone()
        .unwrap_or_else(|| "not set".into());
    let vc_spec = ScreenSpec::new("Correlation Edit")
        .step(3, 3)
        .body_lines([format!("Vercel project (current: {vc_current})")]);
    let vc_input: String = match screen_input(
        &screen,
        &vc_spec,
        "New value (project ID)",
        corr.vercel_project.as_deref(),
        true,
    )? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => String::new(),
    };

    let vc_team_current = corr.vercel_team.clone().unwrap_or_else(|| "not set".into());
    let vc_team_spec = ScreenSpec::new("Correlation Edit")
        .step(3, 3)
        .body_lines([format!("Vercel team (current: {vc_team_current})")]);
    let vc_team_input: String = match screen_input(
        &screen,
        &vc_team_spec,
        "New value (team name or slug)",
        corr.vercel_team.as_deref(),
        true,
    )? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => String::new(),
    };

    // Build updated correlation.
    let updated = CorrelationConfig {
        name: corr.name.clone(),
        github_repo: normalize_input(gh_input),
        railway_project: normalize_input(rw_input),
        railway_workspace: normalize_input(rw_ws_input),
        railway_environment: corr.railway_environment.clone(),
        vercel_project: normalize_input(vc_input),
        vercel_team: normalize_input(vc_team_input),
        branch_mapping: corr.branch_mapping.clone(),
    };

    // Show summary and confirm.
    println!();
    println!("Updated correlation for '{}':", updated.name);
    if let Some(ref v) = updated.github_repo {
        println!("  GitHub:  {v}");
    } else {
        println!("  GitHub:  (not set)");
    }
    if let Some(ref v) = updated.railway_project {
        let ws = updated.railway_workspace.as_deref().unwrap_or("");
        println!("  Railway: {v}  workspace: {ws}");
    } else {
        println!("  Railway: (not set)");
    }
    if let Some(ref v) = updated.vercel_project {
        let team = updated.vercel_team.as_deref().unwrap_or("");
        println!("  Vercel:  {v}  team: {team}");
    } else {
        println!("  Vercel:  (not set)");
    }

    println!();
    let save_spec = ScreenSpec::new("Correlation Edit")
        .body_lines(["Save this correlation update?"]);
    let save = match screen_confirm(&screen, &save_spec, "Save?", true)? {
        PromptResult {
            cancelled: true, ..
        } => false,
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => false,
    };

    if !save {
        println!("Cancelled. No changes saved.");
        return Ok(());
    }

    // Upsert into config.
    match config
        .correlations
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(name))
    {
        Some(existing) => *existing = updated,
        None => config.correlations.push(updated),
    }

    populate_platform_sections(&mut config);
    save_config(&config, config_path)?;
    println!("Saved. Run `pulsos status` to verify.");

    Ok(())
}

/// Normalize interactive text input.
/// Empty string → keep as None. "-" → explicitly clear to None.
fn normalize_input(s: String) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ── Helpers ──

fn parse_resource(resource: &str) -> Result<(String, &str)> {
    let parts: Vec<&str> = resource.splitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!(
            "Invalid resource format '{resource}'. Expected platform:id (e.g., github:myorg/repo)"
        );
    }
    let platform = parts[0].to_lowercase();
    if !["github", "railway", "vercel"].contains(&platform.as_str()) {
        anyhow::bail!("Unknown platform '{platform}'. Use github, railway, or vercel.");
    }
    Ok((platform, parts[1]))
}

fn derive_name(platform: &str, id: &str) -> String {
    match platform {
        "github" => {
            // "myorg/my-saas" → "my-saas"
            id.split('/').next_back().unwrap_or(id).to_string()
        }
        _ => id.to_string(),
    }
}

// ── Groups ──

fn groups_command(cmd: Option<GroupsCommand>, config_path: Option<&Path>) -> Result<()> {
    match cmd.unwrap_or(GroupsCommand::List) {
        GroupsCommand::List => groups_list(config_path),
        GroupsCommand::Create { name, resources } => groups_create(&name, resources, config_path),
        GroupsCommand::Delete { name } => groups_delete(&name, config_path),
    }
}

fn groups_list(config_path: Option<&Path>) -> Result<()> {
    let config = load_config(config_path).unwrap_or_default();

    if config.groups.is_empty() {
        println!("No groups configured. Use `pulsos repos groups create <name> -- <resources...>` to add one.");
        return Ok(());
    }

    println!("{:<24}  Resources", "Name");
    println!("{}", "─".repeat(60));
    for g in &config.groups {
        let resources = if g.resources.is_empty() {
            "(empty)".to_string()
        } else {
            g.resources.join(", ")
        };
        println!("{:<24}  {}", g.name, resources);
    }

    Ok(())
}

fn groups_create(name: &str, resources: Vec<String>, config_path: Option<&Path>) -> Result<()> {
    let mut config = load_config(config_path).unwrap_or_default();

    if config
        .groups
        .iter()
        .any(|g| g.name.eq_ignore_ascii_case(name))
    {
        anyhow::bail!("A group named '{name}' already exists.");
    }

    let count = resources.len();
    config.groups.push(GroupConfig {
        name: name.to_string(),
        resources,
    });

    save_config(&config, config_path)?;
    println!("Group '{name}' created with {count} resource(s).");
    Ok(())
}

fn groups_delete(name: &str, config_path: Option<&Path>) -> Result<()> {
    let screen = ScreenSession::new();
    let mut config =
        load_config(config_path).map_err(|_| anyhow::anyhow!("No configuration found."))?;

    let pos = config
        .groups
        .iter()
        .position(|g| g.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("Group '{name}' not found."))?;

    let confirm_spec = ScreenSpec::new("Delete Group").body_lines([
        format!("This will remove group '{}'.", config.groups[pos].name),
        "This action updates your saved config.".to_string(),
    ]);
    let confirm = match screen_confirm(
        &screen,
        &confirm_spec,
        &format!("Delete group '{}'?", config.groups[pos].name),
        false,
    )? {
        PromptResult {
            cancelled: true, ..
        } => false,
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => false,
    };

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    config.groups.remove(pos);
    save_config(&config, config_path)?;
    println!("Group '{name}' deleted.");
    Ok(())
}

// ── Verify ──

async fn verify_command(config_path: Option<&Path>) -> Result<()> {
    let config = load_config(config_path).map_err(|_| {
        anyhow::anyhow!("No configuration found. Run `pulsos repos sync` to get started.")
    })?;

    if config.correlations.is_empty() {
        println!("No tracked resources. Run `pulsos repos sync` to get started.");
        return Ok(());
    }

    let cache = Arc::new(CacheStore::open_or_temporary());
    let store: Arc<dyn CredentialStore> = Arc::new(FallbackStore::new()?);
    let resolver = TokenResolver::new(store, config.auth.token_detection.clone());
    let health_reports = check_all_platforms_health(&config, &resolver, &cache).await;

    // Resolve tokens and validate platform-level auth once.
    let gh_client = resolver
        .resolve(&PlatformKind::GitHub)
        .map(|t| GitHubClient::new(t, cache.clone()));
    let rw_client = resolver
        .resolve(&PlatformKind::Railway)
        .map(|t| RailwayClient::new(t, cache.clone()));
    let vc_client = resolver
        .resolve(&PlatformKind::Vercel)
        .map(|t| VercelClient::new(t, cache.clone()));

    // Fetch GitHub login once.
    let gh_login = if let Some(ref gh) = gh_client {
        match gh.fetch_user_login().await {
            Ok(login) => Some(login),
            Err(e) => {
                println!("[!!] GitHub auth failed: {}", e.user_message());
                None
            }
        }
    } else {
        let report = health_reports
            .iter()
            .find(|r| r.platform == PlatformKind::GitHub);
        if let Some(report) = report {
            println!("[--] GitHub: {} ({})", report.state.label(), report.reason);
        } else {
            println!("[--] GitHub: no token configured");
        }
        None
    };

    // Validate Railway auth once.
    let rw_ok = if let Some(report) = health_reports
        .iter()
        .find(|r| r.platform == PlatformKind::Railway)
    {
        if report.state == PlatformHealthState::Ready
            || report.state == PlatformHealthState::AccessOrConfigIncomplete
        {
            println!("[ok] Railway: {}", report.reason);
            true
        } else {
            println!("[!!] Railway: {}", report.reason);
            false
        }
    } else {
        println!("[--] Railway: no token configured");
        false
    };

    // Validate Vercel auth once.
    let vc_ok = if let Some(report) = health_reports
        .iter()
        .find(|r| r.platform == PlatformKind::Vercel)
    {
        if report.state == PlatformHealthState::Ready
            || report.state == PlatformHealthState::AccessOrConfigIncomplete
        {
            println!("[ok] Vercel: {}", report.reason);
            true
        } else {
            println!("[!!] Vercel: {}", report.reason);
            false
        }
    } else {
        println!("[--] Vercel: no token configured");
        false
    };

    println!();
    println!("Per-resource checks:");
    println!("{}", "─".repeat(60));

    for corr in &config.correlations {
        println!("  {}:", corr.name);

        // GitHub per-repo permission check.
        if let Some(ref repo) = corr.github_repo {
            if let (Some(ref gh), Some(ref login)) = (&gh_client, &gh_login) {
                let parts: Vec<&str> = repo.splitn(2, '/').collect();
                if parts.len() == 2 {
                    match gh.check_repo_permission(parts[0], parts[1], login).await {
                        Ok(perm) => println!("    [ok] GitHub:{repo}  (permission: {perm})"),
                        Err(e) => println!("    [!!] GitHub:{repo}  {}", e.user_message()),
                    }
                } else {
                    println!("    [!!] GitHub:{repo}  (malformed repo name)");
                }
            } else {
                println!("    [--] GitHub:{repo}  (no token)");
            }
        }

        // Railway — token-level check is sufficient.
        if let Some(ref proj) = corr.railway_project {
            if rw_ok {
                println!("    [ok] Railway:{proj}");
            } else if rw_client.is_none() {
                println!("    [--] Railway:{proj}  (no token)");
            } else {
                println!("    [!!] Railway:{proj}  (auth failed)");
            }
        }

        // Vercel — token-level check is sufficient.
        if let Some(ref proj) = corr.vercel_project {
            if vc_ok {
                println!("    [ok] Vercel:{proj}");
            } else if vc_client.is_none() {
                println!("    [--] Vercel:{proj}  (no token)");
            } else {
                println!("    [!!] Vercel:{proj}  (auth failed)");
            }
        }
    }

    Ok(())
}
