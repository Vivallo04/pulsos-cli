use anyhow::Result;
use clap::{Args, Subcommand};
use pulsos_core::config::{default_config_path, load_config};
use std::path::Path;

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: Option<ConfigCommand>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print current configuration as TOML
    Show,
    /// Print the path to the configuration file
    Path,
    /// Open the configuration file in $EDITOR
    Edit,
    /// Run the interactive platform setup wizard
    Wizard,
}

pub async fn execute(args: ConfigArgs, config_path: Option<&Path>) -> Result<()> {
    match args.command.unwrap_or(ConfigCommand::Show) {
        ConfigCommand::Show => show_config(config_path),
        ConfigCommand::Path => show_path(config_path),
        ConfigCommand::Edit => edit_config(config_path),
        ConfigCommand::Wizard => super::wizard::run_config_wizard(config_path).await,
    }
}

fn show_config(config_path: Option<&Path>) -> Result<()> {
    let config = match load_config(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: failed to load config ({e}); showing defaults.");
            pulsos_core::config::types::PulsosConfig::default()
        }
    };
    let toml = toml::to_string_pretty(&config)
        .map_err(|e| anyhow::anyhow!("Failed to serialize config: {e}"))?;
    print!("{toml}");
    Ok(())
}

fn show_path(config_path: Option<&Path>) -> Result<()> {
    let path = match config_path {
        Some(p) => p.to_path_buf(),
        None => default_config_path()
            .map_err(|e| anyhow::anyhow!("Could not resolve config path: {e}"))?,
    };
    println!("{}", path.display());
    Ok(())
}

fn edit_config(config_path: Option<&Path>) -> Result<()> {
    let path = match config_path {
        Some(p) => p.to_path_buf(),
        None => default_config_path()
            .map_err(|e| anyhow::anyhow!("Could not resolve config path: {e}"))?,
    };

    // Ensure the file exists so the editor has something to open.
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Write the default config if the file doesn't yet exist.
        let default_toml =
            toml::to_string_pretty(&pulsos_core::config::types::PulsosConfig::default())
                .map_err(|e| anyhow::anyhow!("Failed to serialize default config: {e}"))?;
        std::fs::write(&path, default_toml)?;
    }

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to launch editor '{editor}': {e}"))?;

    if !status.success() {
        anyhow::bail!("Editor exited with status: {status}");
    }

    Ok(())
}
