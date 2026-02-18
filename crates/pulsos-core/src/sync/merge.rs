//! Merges new correlations into an existing config, preserving user customizations.

use crate::config::types::{
    CorrelationConfig, OrgConfig, PulsosConfig, TeamConfig, WorkspaceConfig,
};
use std::collections::HashSet;

/// Merge new correlations into an existing config.
///
/// Matching is by name (case-insensitive). For matched entries, platform fields
/// are updated but `branch_mapping` is preserved. New entries are appended.
/// Existing entries not present in `new` are kept (never deleted automatically).
///
/// Returns (merged_config, added_count, updated_count).
pub fn merge_correlations(
    existing: &PulsosConfig,
    new_correlations: Vec<CorrelationConfig>,
) -> (PulsosConfig, usize, usize) {
    let mut config = existing.clone();
    let mut added = 0usize;
    let mut updated = 0usize;

    for new_corr in new_correlations {
        let existing_idx = config
            .correlations
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(&new_corr.name));

        if let Some(idx) = existing_idx {
            // Update platform fields, preserve branch_mapping.
            let existing_corr = &mut config.correlations[idx];

            if new_corr.github_repo.is_some() {
                existing_corr.github_repo = new_corr.github_repo;
            }
            if new_corr.railway_project.is_some() {
                existing_corr.railway_project = new_corr.railway_project;
                existing_corr.railway_workspace = new_corr.railway_workspace;
                existing_corr.railway_environment = new_corr.railway_environment;
            }
            if new_corr.vercel_project.is_some() {
                existing_corr.vercel_project = new_corr.vercel_project;
                existing_corr.vercel_team = new_corr.vercel_team;
            }
            // branch_mapping is NOT overwritten — user customizations preserved.
            updated += 1;
        } else {
            config.correlations.push(new_corr);
            added += 1;
        }
    }

    (config, added, updated)
}

/// Populate the `github.organizations`, `railway.workspaces`, and `vercel.teams`
/// sections from the correlations, so the config is self-consistent.
///
/// Only adds entries that don't already exist in the config.
pub fn populate_platform_sections(config: &mut PulsosConfig) {
    let mut github_orgs: HashSet<String> = config
        .github
        .organizations
        .iter()
        .map(|o| o.name.clone())
        .collect();

    let mut railway_workspaces: HashSet<String> = config
        .railway
        .workspaces
        .iter()
        .map(|w| w.name.clone())
        .collect();

    let mut vercel_teams: HashSet<String> =
        config.vercel.teams.iter().map(|t| t.name.clone()).collect();

    for corr in &config.correlations {
        // GitHub: extract org from "org/repo"
        if let Some(ref repo) = corr.github_repo {
            if let Some(org) = repo.split('/').next() {
                if !org.is_empty() && github_orgs.insert(org.to_string()) {
                    config.github.organizations.push(OrgConfig {
                        name: org.to_string(),
                        include_patterns: vec![],
                        exclude_patterns: vec![],
                        auto_discover: false,
                    });
                }
            }
        }

        // Railway workspace
        if let Some(ref workspace) = corr.railway_workspace {
            if !workspace.is_empty() && railway_workspaces.insert(workspace.clone()) {
                config.railway.workspaces.push(WorkspaceConfig {
                    name: workspace.clone(),
                    id: None,
                    include_projects: vec![],
                    exclude_projects: vec![],
                    default_environment: "production".into(),
                });
            }
        }

        // Vercel team
        if let Some(ref team) = corr.vercel_team {
            if !team.is_empty() && vercel_teams.insert(team.clone()) {
                config.vercel.teams.push(TeamConfig {
                    name: team.clone(),
                    id: None,
                    include_projects: vec![],
                    include_preview_deployments: true,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_correlation(name: &str) -> CorrelationConfig {
        CorrelationConfig {
            name: name.into(),
            github_repo: Some(format!("org/{name}")),
            railway_project: None,
            railway_workspace: None,
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: HashMap::new(),
        }
    }

    #[test]
    fn merge_adds_new_correlations() {
        let existing = PulsosConfig::default();
        let new = vec![test_correlation("my-saas"), test_correlation("api-core")];

        let (config, added, updated) = merge_correlations(&existing, new);
        assert_eq!(added, 2);
        assert_eq!(updated, 0);
        assert_eq!(config.correlations.len(), 2);
    }

    #[test]
    fn merge_updates_existing_by_name() {
        let mut existing = PulsosConfig::default();
        existing.correlations.push(CorrelationConfig {
            name: "my-saas".into(),
            github_repo: Some("org/my-saas".into()),
            railway_project: None,
            railway_workspace: None,
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: HashMap::new(),
        });

        let new = vec![CorrelationConfig {
            name: "my-saas".into(),
            github_repo: Some("org/my-saas".into()),
            railway_project: Some("proj:svc:env".into()),
            railway_workspace: Some("lambda-prod".into()),
            railway_environment: None,
            vercel_project: Some("prj-001".into()),
            vercel_team: Some("Lambda".into()),
            branch_mapping: HashMap::new(),
        }];

        let (config, added, updated) = merge_correlations(&existing, new);
        assert_eq!(added, 0);
        assert_eq!(updated, 1);
        assert_eq!(config.correlations.len(), 1);
        assert_eq!(
            config.correlations[0].railway_project,
            Some("proj:svc:env".into())
        );
        assert_eq!(
            config.correlations[0].vercel_project,
            Some("prj-001".into())
        );
    }

    #[test]
    fn merge_preserves_branch_mappings() {
        let mut existing = PulsosConfig::default();
        let mut branch_map = HashMap::new();
        branch_map.insert("staging".into(), "develop".into());

        existing.correlations.push(CorrelationConfig {
            name: "my-saas".into(),
            github_repo: Some("org/my-saas".into()),
            railway_project: None,
            railway_workspace: None,
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: branch_map,
        });

        let new = vec![CorrelationConfig {
            name: "My-Saas".into(), // case-insensitive match
            github_repo: Some("org/my-saas".into()),
            railway_project: Some("proj:svc:env".into()),
            railway_workspace: Some("lambda-prod".into()),
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: HashMap::new(),
        }];

        let (config, _, _) = merge_correlations(&existing, new);
        assert_eq!(config.correlations[0].branch_mapping.len(), 1);
        assert_eq!(
            config.correlations[0].branch_mapping.get("staging"),
            Some(&"develop".to_string())
        );
    }

    #[test]
    fn merge_keeps_unmatched_existing() {
        let mut existing = PulsosConfig::default();
        existing.correlations.push(test_correlation("old-project"));

        let new = vec![test_correlation("new-project")];
        let (config, added, updated) = merge_correlations(&existing, new);

        assert_eq!(added, 1);
        assert_eq!(updated, 0);
        assert_eq!(config.correlations.len(), 2);
        assert!(config.correlations.iter().any(|c| c.name == "old-project"));
        assert!(config.correlations.iter().any(|c| c.name == "new-project"));
    }

    #[test]
    fn populate_platform_sections_from_correlations() {
        let mut config = PulsosConfig::default();
        config.correlations.push(CorrelationConfig {
            name: "my-saas".into(),
            github_repo: Some("myorg/my-saas".into()),
            railway_project: Some("proj:svc:env".into()),
            railway_workspace: Some("lambda-prod".into()),
            railway_environment: None,
            vercel_project: Some("prj-001".into()),
            vercel_team: Some("Lambda".into()),
            branch_mapping: HashMap::new(),
        });

        populate_platform_sections(&mut config);

        assert_eq!(config.github.organizations.len(), 1);
        assert_eq!(config.github.organizations[0].name, "myorg");

        assert_eq!(config.railway.workspaces.len(), 1);
        assert_eq!(config.railway.workspaces[0].name, "lambda-prod");

        assert_eq!(config.vercel.teams.len(), 1);
        assert_eq!(config.vercel.teams[0].name, "Lambda");
    }

    #[test]
    fn populate_does_not_duplicate_existing_sections() {
        let mut config = PulsosConfig::default();
        config.github.organizations.push(OrgConfig {
            name: "myorg".into(),
            include_patterns: vec!["special-*".into()],
            exclude_patterns: vec![],
            auto_discover: true,
        });
        config.correlations.push(CorrelationConfig {
            name: "my-saas".into(),
            github_repo: Some("myorg/my-saas".into()),
            railway_project: None,
            railway_workspace: None,
            railway_environment: None,
            vercel_project: None,
            vercel_team: None,
            branch_mapping: HashMap::new(),
        });

        populate_platform_sections(&mut config);

        // Should still be 1, not 2
        assert_eq!(config.github.organizations.len(), 1);
        // And the user's patterns should be preserved
        assert_eq!(
            config.github.organizations[0].include_patterns,
            vec!["special-*".to_string()]
        );
    }
}
