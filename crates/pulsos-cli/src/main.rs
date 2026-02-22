mod commands;
mod daemon;
mod output;
mod tui;

use clap::{CommandFactory, Parser, Subcommand};
use output::OutputFormat;
use std::path::PathBuf;

use commands::daemon::{DaemonAction, DaemonArgs};

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
    /// Run a persistent background daemon with a native tray icon
    Daemon(DaemonArgs),
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

// ──────────────────────────────────────────────────────────────────────────────
// Entry point — intentionally NOT #[tokio::main] so that `daemon run` can own
// the OS main thread (required by macOS for the tray event loop).
// ──────────────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    // `daemon run` must own the main thread for the tray icon event loop.
    // Intercept it before starting the Tokio runtime.
    if let Some(Commands::Daemon(DaemonArgs {
        action: DaemonAction::Run,
    })) = &cli.command
    {
        let config_path = cli.config.as_deref();
        let config = load_config_sync(config_path);
        if let Err(e) = commands::daemon::run_daemon_main_thread(config) {
            eprintln!("daemon error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // All other subcommands use a normal multi-thread Tokio runtime.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime")
        .block_on(async_main(cli));
}

/// Load config synchronously (used before the async runtime is started).
fn load_config_sync(
    config_path: Option<&std::path::Path>,
) -> pulsos_core::config::types::PulsosConfig {
    pulsos_core::config::load_config(config_path).unwrap_or_default()
}

async fn async_main(cli: Cli) {
    // Initialize tracing with dual writer (ring buffer + conditional stderr).
    let filter = if cli.verbose {
        tracing_subscriber::EnvFilter::new("pulsos=debug")
    } else {
        tracing_subscriber::EnvFilter::new("pulsos=warn")
    };
    let log_buffer = tui::log_buffer::LogRingBuffer::new();
    let tui_active = tui::log_buffer::TuiActiveFlag::new();
    let writer = tui::log_buffer::DualWriterFactory::new(log_buffer.clone(), tui_active.clone());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(writer)
        .init();

    let config_path = cli.config.as_deref();

    let result = match cli.command {
        Some(Commands::Status(args)) => {
            commands::status::execute(
                args,
                cli.format,
                cli.no_color,
                config_path,
                log_buffer.clone(),
                tui_active.clone(),
            )
            .await
        }
        Some(Commands::Auth(args)) => commands::auth::execute(args, config_path).await,
        Some(Commands::Repos(args)) => commands::repos::execute(args, config_path).await,
        Some(Commands::Views(args)) => commands::views::execute(args, config_path).await,
        Some(Commands::Config(args)) => commands::config::execute(args, config_path).await,
        Some(Commands::Doctor(args)) => commands::doctor::execute(args, config_path).await,
        Some(Commands::Daemon(args)) => commands::daemon::execute(args, config_path).await,
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
            commands::status::execute(
                args,
                cli.format,
                cli.no_color,
                config_path,
                log_buffer,
                tui_active,
            )
            .await
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
