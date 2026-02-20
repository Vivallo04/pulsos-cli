use anyhow::Result;
use crate::commands::ui::screen::{
    screen_confirm, screen_input, screen_multiselect, PromptResult, ScreenSession, ScreenSpec,
};
use clap::{Args, Subcommand};
use pulsos_core::config::types::ViewConfig;
use pulsos_core::config::{default_config_path, load_config, save_config};
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub struct ViewsArgs {
    #[command(subcommand)]
    pub command: Option<ViewsCommand>,
}

#[derive(Debug, Subcommand)]
pub enum ViewsCommand {
    /// List all configured views
    List,
    /// Display view configuration details
    Show { name: String },
    /// Create a new view interactively
    Create,
    /// Delete a view
    Delete { name: String },
    /// Edit an existing view interactively
    Edit { name: String },
    /// List built-in view templates
    Templates,
    /// Validate view projects against correlations
    Validate { name: String },
    /// Export a view to JSON
    Export {
        name: String,
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
    },
    /// Import a view from a JSON file
    Import { file: PathBuf },
}

pub async fn execute(args: ViewsArgs, config_path: Option<&Path>) -> Result<()> {
    match args.command.unwrap_or(ViewsCommand::List) {
        ViewsCommand::List => list_views(config_path),
        ViewsCommand::Show { name } => show_view(&name, config_path),
        ViewsCommand::Create => create_view(config_path),
        ViewsCommand::Delete { name } => delete_view(&name, config_path),
        ViewsCommand::Edit { name } => edit_view(&name, config_path),
        ViewsCommand::Templates => list_templates(),
        ViewsCommand::Validate { name } => validate_view(&name, config_path),
        ViewsCommand::Export { name, output } => export_view(&name, output.as_deref(), config_path),
        ViewsCommand::Import { file } => import_view(&file, config_path),
    }
}

fn list_views(config_path: Option<&Path>) -> Result<()> {
    let config = load_config(config_path).unwrap_or_default();

    if config.views.is_empty() {
        println!("No views configured. Run `pulsos views create` to add one.");
        return Ok(());
    }

    println!(
        "{:<20}  {:<30}  {:<20}  {:<20}  {}",
        "Name", "Description", "Projects", "Platforms", "Branch"
    );
    println!("{}", "─".repeat(100));

    for v in &config.views {
        let desc = v.description.as_deref().unwrap_or("-");
        let projects = if v.projects.is_empty() {
            "all".to_string()
        } else {
            v.projects.join(", ")
        };
        let platforms = if v.platforms.is_empty() {
            "all".to_string()
        } else {
            v.platforms.join(", ")
        };
        let branch = v.branch_filter.as_deref().unwrap_or("-");
        println!(
            "{:<20}  {:<30}  {:<20}  {:<20}  {}",
            v.name, desc, projects, platforms, branch
        );
    }

    Ok(())
}

fn show_view(name: &str, config_path: Option<&Path>) -> Result<()> {
    let config = load_config(config_path).unwrap_or_default();

    let view = config
        .views
        .iter()
        .find(|v| v.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "View '{name}' not found. Run `pulsos views list` to see available views."
            )
        })?;

    println!("View: {}", view.name);
    println!("{}", "─".repeat(44));
    if let Some(ref desc) = view.description {
        println!("  Description:  {desc}");
    }
    if !view.projects.is_empty() {
        println!("  Projects:     {}", view.projects.join(", "));
    } else {
        println!("  Projects:     (all)");
    }
    if !view.platforms.is_empty() {
        println!("  Platforms:    {}", view.platforms.join(", "));
    } else {
        println!("  Platforms:    (all)");
    }
    if let Some(ref branch) = view.branch_filter {
        println!("  Branch:       {branch}");
    }
    if !view.status_filter.is_empty() {
        println!("  Statuses:     {}", view.status_filter.join(", "));
    }
    println!("  Refresh:      {}s", view.refresh_interval);

    Ok(())
}

fn create_view(config_path: Option<&Path>) -> Result<()> {
    let screen = ScreenSession::new();
    let mut config = load_config(config_path).unwrap_or_default();

    println!("Create a new view");
    println!("{}", "─".repeat(44));
    println!("Press Enter to leave a field empty / use defaults.");
    println!();

    // Name (required)
    let name_spec = ScreenSpec::new("Create View")
        .step(1, 4)
        .body_lines(["Enter a name for the new view."]);
    let name: String = match screen_input(&screen, &name_spec, "View name", None, false)? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => String::new(),
    };
    let name = name.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("View name cannot be empty.");
    }
    if config
        .views
        .iter()
        .any(|v| v.name.eq_ignore_ascii_case(&name))
    {
        anyhow::bail!("A view named '{name}' already exists.");
    }

    // Description (optional)
    let desc_spec = ScreenSpec::new("Create View")
        .step(1, 4)
        .body_lines(["Optionally add a description."]);
    let description: String = match screen_input(
        &screen,
        &desc_spec,
        "Description (optional)",
        None,
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
    let description = description.trim().to_string();
    let description = if description.is_empty() {
        None
    } else {
        Some(description)
    };

    // Platforms (multiselect)
    let platform_options = &["github", "railway", "vercel"];
    let platform_defaults: Vec<bool> = vec![false; platform_options.len()];
    let platform_spec = ScreenSpec::new("Create View")
        .step(2, 4)
        .body_lines([
            "Select platforms to include in this view.",
            "All options are disabled by default.",
        ]);
    let platform_selections = match screen_multiselect(
        &screen,
        &platform_spec,
        "Platforms to include",
        platform_options,
        &platform_defaults,
    )? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => vec![],
    };
    let platforms: Vec<String> = platform_selections
        .into_iter()
        .map(|i| platform_options[i].to_string())
        .collect();

    // Projects (freetext, comma-separated)
    let projects_spec = ScreenSpec::new("Create View")
        .step(3, 4)
        .body_lines(["Projects (comma-separated), or empty for all."]);
    let projects_input: String = match screen_input(
        &screen,
        &projects_spec,
        "Projects to include",
        None,
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
    let projects: Vec<String> = projects_input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Branch filter (optional)
    let branch_spec = ScreenSpec::new("Create View")
        .step(3, 4)
        .body_lines(["Optional branch filter (e.g. main, feature/)."]);
    let branch_input: String = match screen_input(
        &screen,
        &branch_spec,
        "Branch filter (optional)",
        None,
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
    let branch_filter = branch_input.trim().to_string();
    let branch_filter = if branch_filter.is_empty() {
        None
    } else {
        Some(branch_filter)
    };

    let view = ViewConfig {
        name: name.clone(),
        description,
        projects,
        platforms,
        branch_filter,
        status_filter: vec![],
        refresh_interval: 5,
        vercel_include_previews: false,
    };

    println!();
    println!("New view summary:");
    println!("  Name:      {}", view.name);
    if let Some(ref d) = view.description {
        println!("  Desc:      {d}");
    }
    if !view.platforms.is_empty() {
        println!("  Platforms: {}", view.platforms.join(", "));
    }
    if !view.projects.is_empty() {
        println!("  Projects:  {}", view.projects.join(", "));
    }
    if let Some(ref b) = view.branch_filter {
        println!("  Branch:    {b}");
    }

    println!();
    let save_spec = ScreenSpec::new("Create View")
        .step(4, 4)
        .body_lines(["Review complete. Save this view?"]);
    let save = match screen_confirm(&screen, &save_spec, "Save this view?", true)? {
        PromptResult {
            cancelled: true, ..
        } => false,
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => false,
    };

    if !save {
        println!("Cancelled.");
        return Ok(());
    }

    config.views.push(view);
    save_config(&config, config_path).map_err(|e| anyhow::anyhow!("Failed to save config: {e}"))?;

    println!("View '{name}' saved. Use `pulsos status --view {name}` to apply it.");
    Ok(())
}

fn delete_view(name: &str, config_path: Option<&Path>) -> Result<()> {
    let screen = ScreenSession::new();
    let mut config =
        load_config(config_path).map_err(|_| anyhow::anyhow!("No configuration found."))?;

    let pos = config
        .views
        .iter()
        .position(|v| v.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("View '{name}' not found."))?;

    println!("This will delete view '{}'.", config.views[pos].name);
    let confirm_spec = ScreenSpec::new("Delete View").body_lines([
        format!("This will delete view '{}'.", config.views[pos].name),
        "This action updates your saved config.".to_string(),
    ]);
    let confirm = match screen_confirm(&screen, &confirm_spec, "Are you sure?", false)? {
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

    config.views.remove(pos);
    save_config(&config, config_path).map_err(|e| anyhow::anyhow!("Failed to save config: {e}"))?;

    println!("View '{name}' deleted.");

    // Show config path hint
    let path = config_path
        .map(|p| p.to_path_buf())
        .or_else(|| default_config_path().ok());
    if let Some(p) = path {
        println!("Config saved to: {}", p.display());
    }

    Ok(())
}

// ── Edit ──

fn edit_view(name: &str, config_path: Option<&Path>) -> Result<()> {
    let screen = ScreenSession::new();
    let mut config =
        load_config(config_path).map_err(|_| anyhow::anyhow!("No configuration found."))?;

    let pos = config
        .views
        .iter()
        .position(|v| v.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("View '{name}' not found."))?;

    let existing = config.views[pos].clone();

    println!("Editing view '{}'", existing.name);
    println!("{}", "─".repeat(44));
    println!("Press Enter to keep the current value.");
    println!();

    // Name
    let new_name_spec = ScreenSpec::new("Edit View")
        .step(1, 4)
        .body_lines(["Update view name."]);
    let new_name: String = match screen_input(
        &screen,
        &new_name_spec,
        "View name",
        Some(&existing.name),
        false,
    )? {
        PromptResult {
            cancelled: true, ..
        } => return Ok(()),
        PromptResult {
            value: Some(value), ..
        } => value,
        _ => String::new(),
    };
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        anyhow::bail!("View name cannot be empty.");
    }
    if !new_name.eq_ignore_ascii_case(&existing.name)
        && config
            .views
            .iter()
            .any(|v| v.name.eq_ignore_ascii_case(&new_name))
    {
        anyhow::bail!("A view named '{new_name}' already exists.");
    }

    // Description
    let desc_current = existing.description.as_deref().unwrap_or("");
    let desc_spec = ScreenSpec::new("Edit View")
        .step(1, 4)
        .body_lines(["Update description (optional)."]);
    let new_desc: String = match screen_input(
        &screen,
        &desc_spec,
        "Description (optional)",
        Some(desc_current),
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
    let new_desc = new_desc.trim().to_string();
    let new_description = if new_desc.is_empty() {
        None
    } else {
        Some(new_desc)
    };

    // Platforms
    let platform_options = &["github", "railway", "vercel"];
    let defaults: Vec<bool> = platform_options
        .iter()
        .map(|p| existing.platforms.iter().any(|ep| ep == p))
        .collect();
    let platform_spec = ScreenSpec::new("Edit View")
        .step(2, 4)
        .body_lines(["Update included platforms."]);
    let platform_selections = match screen_multiselect(
        &screen,
        &platform_spec,
        "Platforms to include",
        platform_options,
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
    let new_platforms: Vec<String> = platform_selections
        .into_iter()
        .map(|i| platform_options[i].to_string())
        .collect();

    // Projects
    let projects_current = existing.projects.join(", ");
    let projects_spec = ScreenSpec::new("Edit View")
        .step(3, 4)
        .body_lines(["Update projects (comma-separated), or empty for all."]);
    let projects_input: String = match screen_input(
        &screen,
        &projects_spec,
        "Projects to include",
        Some(&projects_current),
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
    let new_projects: Vec<String> = projects_input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Branch filter
    let branch_current = existing.branch_filter.as_deref().unwrap_or("");
    let branch_spec = ScreenSpec::new("Edit View")
        .step(3, 4)
        .body_lines(["Update optional branch filter."]);
    let branch_input: String = match screen_input(
        &screen,
        &branch_spec,
        "Branch filter (optional)",
        Some(branch_current),
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
    let new_branch = branch_input.trim().to_string();
    let new_branch_filter = if new_branch.is_empty() {
        None
    } else {
        Some(new_branch)
    };

    let updated = ViewConfig {
        name: new_name.clone(),
        description: new_description,
        projects: new_projects,
        platforms: new_platforms,
        branch_filter: new_branch_filter,
        status_filter: existing.status_filter.clone(),
        refresh_interval: existing.refresh_interval,
        vercel_include_previews: existing.vercel_include_previews,
    };

    println!();
    println!("Updated view summary:");
    println!("  Name:      {}", updated.name);
    if let Some(ref d) = updated.description {
        println!("  Desc:      {d}");
    }
    if !updated.platforms.is_empty() {
        println!("  Platforms: {}", updated.platforms.join(", "));
    }
    if !updated.projects.is_empty() {
        println!("  Projects:  {}", updated.projects.join(", "));
    }
    if let Some(ref b) = updated.branch_filter {
        println!("  Branch:    {b}");
    }

    println!();
    let save_spec = ScreenSpec::new("Edit View")
        .step(4, 4)
        .body_lines(["Save these changes?"]);
    let save = match screen_confirm(&screen, &save_spec, "Save changes?", true)? {
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

    config.views[pos] = updated;
    save_config(&config, config_path).map_err(|e| anyhow::anyhow!("Failed to save config: {e}"))?;

    println!("View '{new_name}' updated.");
    Ok(())
}

// ── Templates ──

fn list_templates() -> Result<()> {
    struct Template {
        name: &'static str,
        description: &'static str,
        platforms: &'static str,
        branch: &'static str,
    }

    let templates = [
        Template {
            name: "full-stack-production",
            description: "End-to-end production monitoring",
            platforms: "github, railway, vercel",
            branch: "main",
        },
        Template {
            name: "backend-infrastructure",
            description: "Backend team daily driver",
            platforms: "github, railway",
            branch: "(all branches)",
        },
        Template {
            name: "frontend-releases",
            description: "Frontend team deployments",
            platforms: "github, vercel",
            branch: "(all branches)",
        },
        Template {
            name: "security-monitoring",
            description: "Security audit dashboard",
            platforms: "github, vercel",
            branch: "main",
        },
    ];

    println!("Built-in View Templates");
    println!("{}", "─".repeat(70));

    for t in &templates {
        println!();
        println!("  {}", t.name);
        println!("  Description: {}", t.description);
        println!("  Platforms:   {}", t.platforms);
        println!("  Branch:      {}", t.branch);
        println!("  Usage hint:  pulsos views create  (enter values above)");
    }

    println!();
    Ok(())
}

// ── Validate ──

fn validate_view(name: &str, config_path: Option<&Path>) -> Result<()> {
    let config =
        load_config(config_path).map_err(|_| anyhow::anyhow!("No configuration found."))?;

    let view = config
        .views
        .iter()
        .find(|v| v.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("View '{name}' not found."))?;

    println!("Validating view '{}'", view.name);
    println!("{}", "─".repeat(44));

    let mut ok_count = 0;
    let mut err_count = 0;

    if view.projects.is_empty() {
        // "all projects" mode — check each correlation has at least one platform
        for corr in &config.correlations {
            let has_platform = corr.github_repo.is_some()
                || corr.railway_project.is_some()
                || corr.vercel_project.is_some();
            if has_platform {
                println!("  [ok] {}", corr.name);
                ok_count += 1;
            } else {
                println!("  [!!] {} — no platform fields set", corr.name);
                err_count += 1;
            }
        }
        if config.correlations.is_empty() {
            println!("  (no correlations to validate)");
        }
    } else {
        // Explicit project list — check each exists in correlations
        for project_name in &view.projects {
            let found = config
                .correlations
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(project_name));
            if found {
                println!("  [ok] {project_name}");
                ok_count += 1;
            } else {
                println!("  [!!] {project_name} — not found in correlations");
                err_count += 1;
            }
        }
    }

    println!();
    println!("  Summary: {ok_count} ok, {err_count} issue(s)");

    Ok(())
}

// ── Export ──

fn export_view(name: &str, output: Option<&Path>, config_path: Option<&Path>) -> Result<()> {
    let config =
        load_config(config_path).map_err(|_| anyhow::anyhow!("No configuration found."))?;

    let view = config
        .views
        .iter()
        .find(|v| v.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("View '{name}' not found."))?;

    let json = serde_json::to_string_pretty(view)
        .map_err(|e| anyhow::anyhow!("Failed to serialize view: {e}"))?;

    if let Some(path) = output {
        std::fs::write(path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write file '{}': {e}", path.display()))?;
        println!("View '{}' exported to '{}'.", view.name, path.display());
    } else {
        println!("{json}");
    }

    Ok(())
}

// ── Import ──

fn import_view(file: &Path, config_path: Option<&Path>) -> Result<()> {
    let content = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {e}", file.display()))?;

    let view: ViewConfig = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse view JSON: {e}"))?;

    let mut config = load_config(config_path).unwrap_or_default();

    if config
        .views
        .iter()
        .any(|v| v.name.eq_ignore_ascii_case(&view.name))
    {
        anyhow::bail!("A view named '{}' already exists.", view.name);
    }

    let view_name = view.name.clone();
    config.views.push(view);
    save_config(&config, config_path).map_err(|e| anyhow::anyhow!("Failed to save config: {e}"))?;

    println!("View '{view_name}' imported.");
    Ok(())
}
