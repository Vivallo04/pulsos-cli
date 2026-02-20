mod commands;
mod output;
mod tui;

use clap::{CommandFactory, Parser, Subcommand};
use output::OutputFormat;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "pulsos",
    about = "Unified deployment monitoring across GitHub Actions, Railway, and Vercel",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Output format
    #[arg(long, global = true, default_value = "table")]
    format: OutputFormat,

    /// Disable color output
    #[arg(long, global = true)]
    no_color: bool,

    /// Show debug information
    #[arg(long, global = true)]
    verbose: bool,

    /// Custom config file path
    #[arg(long, global = true)]
    config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Show deployment status across all platforms
    Status(commands::status::StatusArgs),
    /// Manage authentication
    Auth(commands::auth::AuthArgs),
    /// Manage tracked repositories and projects
    Repos(commands::repos::ReposArgs),
    /// Manage saved views
    Views(commands::views::ViewsArgs),
    /// View and edit configuration
    Config(commands::config::ConfigArgs),
    /// Diagnostics and troubleshooting
    Doctor(commands::doctor::DoctorArgs),
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.verbose {
        tracing_subscriber::EnvFilter::new("pulsos=debug")
    } else {
        tracing_subscriber::EnvFilter::new("pulsos=warn")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let config_path = cli.config.as_deref();

    let result = match cli.command {
        Some(Commands::Status(args)) => {
            commands::status::execute(args, cli.format, cli.no_color, config_path).await
        }
        Some(Commands::Auth(args)) => commands::auth::execute(args, config_path).await,
        Some(Commands::Repos(args)) => commands::repos::execute(args, config_path).await,
        Some(Commands::Views(args)) => commands::views::execute(args, config_path).await,
        Some(Commands::Config(args)) => commands::config::execute(args, config_path).await,
        Some(Commands::Doctor(args)) => commands::doctor::execute(args, config_path).await,
        Some(Commands::Completions { shell }) => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "pulsos", &mut std::io::stdout());
            Ok(())
        }
        None => {
            // Default: run status
            let args = commands::status::StatusArgs {
                project: None,
                platform: None,
                view: None,
                branch: None,
                watch: false,
                once: false,
            };
            commands::status::execute(args, cli.format, cli.no_color, config_path).await
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli_structure() {
        Cli::command().debug_assert();
    }
}
