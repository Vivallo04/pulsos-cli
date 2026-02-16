use clap::{Args, Subcommand};

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

pub async fn execute(_args: ReposArgs) -> anyhow::Result<()> {
    eprintln!("Repos commands are not yet implemented. Coming in Phase 2.");
    Ok(())
}
