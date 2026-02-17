use anyhow::Result;
use clap::{Args, Subcommand};
use pulsos_core::auth::credential_store::{CredentialStore, KeyringStore};
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::validate::validate_token;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::load_config;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: Option<AuthCommand>,

    /// Only check environment variables (skip keyring and interactive prompts). For CI/CD.
    #[arg(long, global = true)]
    pub from_env: bool,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Check auth status across platforms
    Status,
    /// Authenticate with GitHub
    Github,
    /// Authenticate with Railway
    Railway,
    /// Authenticate with Vercel
    Vercel,
    /// Logout from one or all platforms
    Logout {
        /// Platform to logout from (github, railway, vercel). Prompts if not specified.
        #[arg(long)]
        platform: Option<String>,
    },
}

pub async fn execute(args: AuthArgs, config_path: Option<&Path>) -> Result<()> {
    let config = load_config(config_path).unwrap_or_default();
    let cache = Arc::new(CacheStore::open_default()?);

    let store: Arc<dyn CredentialStore> = if args.from_env {
        // In --from-env mode, use an empty in-memory store so keyring is never accessed
        Arc::new(pulsos_core::auth::credential_store::InMemoryStore::new())
    } else {
        Arc::new(KeyringStore::new())
    };

    let resolver = TokenResolver::new(store.clone(), config.auth.token_detection.clone());

    match args.command {
        None | Some(AuthCommand::Status) => auth_status(&resolver, &cache).await,
        Some(AuthCommand::Github) => {
            auth_platform(PlatformKind::GitHub, &store, &cache, args.from_env).await
        }
        Some(AuthCommand::Railway) => {
            auth_platform(PlatformKind::Railway, &store, &cache, args.from_env).await
        }
        Some(AuthCommand::Vercel) => {
            auth_platform(PlatformKind::Vercel, &store, &cache, args.from_env).await
        }
        Some(AuthCommand::Logout { platform }) => auth_logout(platform, &store).await,
    }
}

/// Show auth status for all platforms.
async fn auth_status(resolver: &TokenResolver, cache: &Arc<CacheStore>) -> Result<()> {
    println!("Authentication Status");
    println!("{}", "=".repeat(44));
    println!();

    for platform in &PlatformKind::ALL {
        print!("  {:<12}", format!("{}:", platform.display_name()));

        match resolver.resolve_with_source(platform) {
            Some((token, source)) => match validate_token(platform, &token, cache).await {
                Ok(status) => {
                    println!("OK via {} ({})", source, status.identity);
                    for warning in &status.warnings {
                        println!("  {:<12}  warning: {}", "", warning);
                    }
                }
                Err(e) => {
                    println!("FAIL via {} - {}", source, e);
                }
            },
            None => {
                println!("not configured. Run `pulsos auth {}`", platform.cli_name());
            }
        }
    }

    println!();
    Ok(())
}

/// Interactive authentication for a single platform.
async fn auth_platform(
    platform: PlatformKind,
    store: &Arc<dyn CredentialStore>,
    cache: &Arc<CacheStore>,
    from_env: bool,
) -> Result<()> {
    println!("Authenticating with {}", platform.display_name());
    println!();

    // Check if a token already exists
    if let Ok(Some(_)) = store.get(&platform) {
        println!(
            "  A token is already stored in the keyring for {}.",
            platform.display_name()
        );
        println!("  Continuing will replace it.");
        println!();
    }

    // In --from-env mode, just check env vars
    if from_env {
        return auth_from_env(&platform, cache).await;
    }

    // Show help text for getting a token
    print_token_help(&platform);

    // Prompt for token
    let token: String = dialoguer::Password::new()
        .with_prompt(format!("  Enter your {} token", platform.display_name()))
        .interact()?;

    if token.is_empty() {
        anyhow::bail!("No token provided. Authentication cancelled.");
    }

    // Validate the token
    print!("  Validating... ");
    match validate_token(&platform, &token, cache).await {
        Ok(status) => {
            println!("OK ({})", status.identity);

            // Store in keyring
            store.set(&platform, &token).map_err(|e| {
                anyhow::anyhow!("Token is valid but failed to store in keyring: {e}")
            })?;

            println!(
                "  Token stored securely in your OS keyring for {}.",
                platform.display_name()
            );

            // Show any warnings
            for warning in &status.warnings {
                println!("  warning: {warning}");
            }
        }
        Err(e) => {
            println!("FAILED");
            println!();
            println!("  {e}");
            println!();
            println!("  Token was NOT stored. Please check your token and try again.");
            return Err(e.into());
        }
    }

    println!();
    Ok(())
}

/// Validate token from environment variable only.
async fn auth_from_env(platform: &PlatformKind, cache: &Arc<CacheStore>) -> Result<()> {
    for var_name in platform.env_var_names() {
        if let Ok(token) = std::env::var(var_name) {
            if !token.is_empty() {
                print!("  Found {var_name}. Validating... ");
                match validate_token(platform, &token, cache).await {
                    Ok(status) => {
                        println!("OK ({})", status.identity);
                        for warning in &status.warnings {
                            println!("  warning: {warning}");
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        println!("FAILED - {e}");
                        return Err(e.into());
                    }
                }
            }
        }
    }

    let vars = platform.env_var_names().join(" or ");
    anyhow::bail!(
        "No {} token found in environment. Set {} before using --from-env.",
        platform.display_name(),
        vars
    );
}

/// Logout from one or all platforms.
async fn auth_logout(
    platform_name: Option<String>,
    store: &Arc<dyn CredentialStore>,
) -> Result<()> {
    let platforms: Vec<PlatformKind> = if let Some(name) = platform_name {
        let p = match name.to_ascii_lowercase().as_str() {
            "github" => PlatformKind::GitHub,
            "railway" => PlatformKind::Railway,
            "vercel" => PlatformKind::Vercel,
            other => {
                anyhow::bail!("Unknown platform: {other}. Use 'github', 'railway', or 'vercel'.")
            }
        };
        vec![p]
    } else {
        // Prompt which platform to logout from
        let items = &["GitHub", "Railway", "Vercel", "All platforms"];
        let selection = dialoguer::Select::new()
            .with_prompt("Which platform do you want to logout from?")
            .items(items)
            .default(0)
            .interact()?;

        match selection {
            0 => vec![PlatformKind::GitHub],
            1 => vec![PlatformKind::Railway],
            2 => vec![PlatformKind::Vercel],
            3 => PlatformKind::ALL.to_vec(),
            _ => unreachable!(),
        }
    };

    for platform in &platforms {
        match store.delete(platform) {
            Ok(()) => {
                println!("  Removed {} token from keyring.", platform.display_name());
            }
            Err(e) => {
                println!(
                    "  Failed to remove {} token: {}",
                    platform.display_name(),
                    e
                );
            }
        }
    }

    // Remind about env vars
    let env_vars: Vec<&str> = platforms
        .iter()
        .flat_map(|p| p.env_var_names().iter())
        .copied()
        .collect();

    let set_vars: Vec<&&str> = env_vars
        .iter()
        .filter(|v| std::env::var(v).is_ok_and(|t| !t.is_empty()))
        .collect();

    if !set_vars.is_empty() {
        println!();
        println!("  Note: The following environment variables are still set:");
        for var in &set_vars {
            println!("    {var}");
        }
        println!("  Pulsos cannot remove environment variables. Unset them manually if needed.");
    }

    println!();
    Ok(())
}

/// Print instructions for getting a token for the given platform.
fn print_token_help(platform: &PlatformKind) {
    match platform {
        PlatformKind::GitHub => {
            println!("  Create a personal access token at:");
            println!("    https://github.com/settings/tokens");
            println!();
            println!("  Required scopes: repo, read:org");
            println!();
        }
        PlatformKind::Railway => {
            println!("  Create an API token at:");
            println!("    https://railway.com/account/tokens");
            println!();
        }
        PlatformKind::Vercel => {
            println!("  Create a token at:");
            println!("    https://vercel.com/account/tokens");
            println!();
        }
    }
}
