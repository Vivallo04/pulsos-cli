pub mod types;
pub mod validate;

use crate::error::PulsosError;
use std::path::{Path, PathBuf};
use types::PulsosConfig;

/// Return the default config file path: ~/.config/pulsos/config.toml
pub fn default_config_path() -> Result<PathBuf, PulsosError> {
    dirs::config_dir()
        .map(|d| d.join("pulsos").join("config.toml"))
        .ok_or_else(|| PulsosError::Config("Could not determine config directory".into()))
}

/// Load config from the default path or a custom path.
pub fn load_config(path: Option<&Path>) -> Result<PulsosConfig, PulsosError> {
    let config_path = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path()?,
    };

    if !config_path.exists() {
        return Err(PulsosError::NoConfig);
    }

    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        PulsosError::Config(format!("Failed to read {}: {e}", config_path.display()))
    })?;

    let config: PulsosConfig = toml::from_str(&content).map_err(|e| {
        PulsosError::Config(format!("Failed to parse {}: {e}", config_path.display()))
    })?;

    Ok(config)
}

/// Save config to the default path or a custom path.
pub fn save_config(config: &PulsosConfig, path: Option<&Path>) -> Result<(), PulsosError> {
    let config_path = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path()?,
    };

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            PulsosError::Config(format!(
                "Failed to create directory {}: {e}",
                parent.display()
            ))
        })?;
    }

    let content = toml::to_string_pretty(config)
        .map_err(|e| PulsosError::Config(format!("Failed to serialize config: {e}")))?;

    std::fs::write(&config_path, content).map_err(|e| {
        PulsosError::Config(format!("Failed to write {}: {e}", config_path.display()))
    })?;

    Ok(())
}
