use super::types::PulsosConfig;
use crate::error::PulsosError;

/// Validate a loaded configuration.
pub fn validate_config(config: &PulsosConfig) -> Result<(), PulsosError> {
    // Check that at least one platform has some tracked resources
    let has_github = !config.github.organizations.is_empty();
    let has_railway = !config.railway.workspaces.is_empty();
    let has_vercel = !config.vercel.teams.is_empty();

    if !has_github && !has_railway && !has_vercel {
        return Err(PulsosError::Config(
            "No platforms configured. Run `pulsos repos sync` to discover and select resources."
                .into(),
        ));
    }

    // Validate view references
    for view in &config.views {
        if view.name.is_empty() {
            return Err(PulsosError::Config("View name cannot be empty.".into()));
        }
    }

    // Validate correlation references
    for corr in &config.correlations {
        if corr.name.is_empty() {
            return Err(PulsosError::Config(
                "Correlation name cannot be empty.".into(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::OrgConfig;

    #[test]
    fn empty_config_fails_validation() {
        let config = PulsosConfig::default();
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No platforms"));
    }

    #[test]
    fn config_with_github_passes() {
        let mut config = PulsosConfig::default();
        config.github.organizations.push(OrgConfig {
            name: "myorg".into(),
            include_patterns: vec![],
            exclude_patterns: vec![],
            auto_discover: true,
        });
        assert!(validate_config(&config).is_ok());
    }
}
