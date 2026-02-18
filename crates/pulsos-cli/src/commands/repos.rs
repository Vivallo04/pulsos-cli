use anyhow::Result;
use clap::{Args, Subcommand};
use pulsos_core::auth::credential_store::KeyringStore;
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::types::CorrelationConfig;
use pulsos_core::config::{load_config, save_config};
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
}

pub async fn execute(args: ReposArgs, config_path: Option<&Path>) -> Result<()> {
    match args.command {
        Some(ReposCommand::Sync) | None => sync_command(config_path).await,
        Some(ReposCommand::List) => list_command(config_path),
        Some(ReposCommand::Add { resource }) => add_command(&resource, config_path),
        Some(ReposCommand::Remove { resource }) => remove_command(&resource, config_path),
    }
}

async fn sync_command(config_path: Option<&Path>) -> Result<()> {
    println!("Discovering projects across platforms...");
    println!();

    // Load existing config or start fresh.
    let existing_config = load_config(config_path).unwrap_or_default();

    let cache = Arc::new(CacheStore::open_default()?);
    let store = Arc::new(KeyringStore::new());
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
        let defaults: Vec<bool> = vec![true; items.len()];
        let selections = dialoguer::MultiSelect::new()
            .with_prompt("Select GitHub repositories to track")
            .items(&items)
            .defaults(&defaults)
            .interact()?;
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
        let defaults: Vec<bool> = vec![true; items.len()];
        let selections = dialoguer::MultiSelect::new()
            .with_prompt("Select Railway services to track")
            .items(&items)
            .defaults(&defaults)
            .interact()?;
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
        let defaults: Vec<bool> = vec![true; items.len()];
        let selections = dialoguer::MultiSelect::new()
            .with_prompt("Select Vercel projects to track")
            .items(&items)
            .defaults(&defaults)
            .interact()?;
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
    let confirm = dialoguer::Confirm::new()
        .with_prompt("Accept these correlations?")
        .default(true)
        .interact()?;

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
