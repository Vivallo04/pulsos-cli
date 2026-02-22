use crate::commands::ui::screen::{
    screen_confirm, screen_password_masked, screen_select, PromptResult, ScreenSession,
    ScreenSeverity, ScreenSpec,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal,
};
use pulsos_core::auth::credential_store::{CredentialStore, FallbackStore, InMemoryStore};
use pulsos_core::auth::detect;
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::validate::validate_token;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::load_config;
use pulsos_core::config::types::PulsosConfig;
use pulsos_core::health::check_all_platforms_health;
use secrecy::SecretString;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: Option<AuthCommand>,

    /// Only check environment variables (skip keyring and interactive prompts). For CI/CD.
    #[arg(long, global = true)]
    pub from_env: bool,

    /// CI/CD mode: store provided tokens non-interactively and exit.
    /// Use with --github-token, --railway-token, and/or --vercel-token.
    #[arg(long, conflicts_with = "from_env")]
    pub ci: bool,

    /// GitHub token to store (requires --ci)
    #[arg(long, value_name = "TOKEN", requires = "ci")]
    pub github_token: Option<String>,

    /// Railway token to store (requires --ci)
    #[arg(long, value_name = "TOKEN", requires = "ci")]
    pub railway_token: Option<String>,

    /// Vercel token to store (requires --ci)
    #[arg(long, value_name = "TOKEN", requires = "ci")]
    pub vercel_token: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Check auth status across platforms
    Status,
    /// Authenticate with GitHub
    Github {
        /// Provide token directly (non-interactive)
        #[arg(long)]
        token: Option<String>,
    },
    /// Authenticate with Railway
    Railway {
        /// Provide token directly (non-interactive)
        #[arg(long)]
        token: Option<String>,
    },
    /// Authenticate with Vercel
    Vercel {
        /// Provide token directly (non-interactive)
        #[arg(long)]
        token: Option<String>,
    },
    /// Logout from one or all platforms
    Logout {
        /// Platform to logout from (github, railway, vercel). Prompts if not specified.
        #[arg(long)]
        platform: Option<String>,
    },
    /// Re-validate stored tokens; re-authenticate if invalid
    Refresh {
        /// Platform to refresh (github, railway, vercel). Refreshes all if omitted.
        #[arg(long)]
        platform: Option<String>,
    },
}

pub async fn execute(args: AuthArgs, config_path: Option<&Path>) -> Result<()> {
    let config = load_config(config_path).unwrap_or_default();
    let cache = Arc::new(CacheStore::open_or_temporary()?);

    let store: Arc<dyn CredentialStore> = if args.from_env {
        Arc::new(InMemoryStore::new())
    } else {
        Arc::new(FallbackStore::new()?)
    };

    let resolver = TokenResolver::new(store.clone(), config.auth.token_detection.clone());

    if args.ci {
        return auth_ci(
            args.github_token,
            args.railway_token,
            args.vercel_token,
            &store,
            &cache,
        )
        .await;
    }

    let screen = ScreenSession::new();

    match args.command {
        None | Some(AuthCommand::Status) => auth_status(&config, &resolver, &cache).await,
        Some(AuthCommand::Github { token }) => {
            auth_platform(
                PlatformKind::GitHub,
                &store,
                &cache,
                args.from_env,
                token,
                Some(&screen),
                Some((1, 1)),
            )
            .await
        }
        Some(AuthCommand::Railway { token }) => {
            auth_platform(
                PlatformKind::Railway,
                &store,
                &cache,
                args.from_env,
                token,
                Some(&screen),
                Some((1, 1)),
            )
            .await
        }
        Some(AuthCommand::Vercel { token }) => {
            auth_platform(
                PlatformKind::Vercel,
                &store,
                &cache,
                args.from_env,
                token,
                Some(&screen),
                Some((1, 1)),
            )
            .await
        }
        Some(AuthCommand::Logout { platform }) => {
            auth_logout(platform, &store, Some(&screen)).await
        }
        Some(AuthCommand::Refresh { platform }) => {
            auth_refresh(
                platform,
                &resolver,
                &store,
                &cache,
                args.from_env,
                Some(&screen),
            )
            .await
        }
    }
}

fn platform_display_name(platform: PlatformKind) -> &'static str {
    match platform {
        PlatformKind::GitHub => "GitHub",
        PlatformKind::Railway => "Railway",
        PlatformKind::Vercel => "Vercel",
    }
}

// ── CI/CD one-shot ────────────────────────────────────────────────────────────

async fn auth_ci(
    github_token: Option<String>,
    railway_token: Option<String>,
    vercel_token: Option<String>,
    store: &Arc<dyn CredentialStore>,
    cache: &Arc<CacheStore>,
) -> Result<()> {
    let pairs = [
        (PlatformKind::GitHub, github_token),
        (PlatformKind::Railway, railway_token),
        (PlatformKind::Vercel, vercel_token),
    ];

    let any_provided = pairs.iter().any(|(_, t)| t.is_some());
    if !any_provided {
        anyhow::bail!(
            "--ci requires at least one token flag: \
             --github-token, --railway-token, or --vercel-token"
        );
    }

    let mut failed = false;
    for (platform, token) in pairs {
        if let Some(t) = token {
            if let Err(e) = auth_platform(platform, store, cache, false, Some(t), None, None).await
            {
                eprintln!("  {}: FAILED — {e}", platform_display_name(platform));
                failed = true;
            }
        }
    }

    if failed {
        anyhow::bail!("One or more token validations failed.");
    }
    Ok(())
}

// ── Auth status ───────────────────────────────────────────────────────────────

async fn auth_status(
    config: &PulsosConfig,
    resolver: &TokenResolver,
    cache: &Arc<CacheStore>,
) -> Result<()> {
    println!("Authentication Status");
    println!("{}", "=".repeat(44));
    println!();

    let reports = check_all_platforms_health(config, resolver, cache).await;
    for report in reports {
        let source = resolver
            .resolve_with_source(&report.platform)
            .map(|(_, src)| src.to_string())
            .unwrap_or_else(|| "none".to_string());
        println!(
            "  {:<12} {} {:<16} via {}",
            format!("{}:", report.platform.display_name()),
            report.state.icon(),
            report.state.label(),
            source
        );
        println!("  {:<12}  {}", "", report.reason);
        if !report.next_action.is_empty() && report.next_action != "No action needed." {
            println!("  {:<12}  next: {}", "", report.next_action);
        }
    }

    println!();
    Ok(())
}

// ── Interactive / direct platform auth ───────────────────────────────────────

/// Authenticate a single platform interactively (or via direct `--token` flag).
///
/// The step header is NOT printed here — callers are responsible for printing
/// `print_wizard_step_header` or `print_step_header_plain` before calling this.
pub(crate) async fn auth_platform(
    platform: PlatformKind,
    store: &Arc<dyn CredentialStore>,
    cache: &Arc<CacheStore>,
    from_env: bool,
    token: Option<String>,
    screen: Option<&ScreenSession>,
    step: Option<(usize, usize)>,
) -> Result<()> {
    let base_spec = auth_screen_spec(platform, step, token_help_lines(&platform), vec![]);

    // ── Direct token path (--token flag) ──────────────────────────────────────
    if let Some(token_str) = token {
        let token_str = token_str.trim().to_string();
        if token_str.is_empty() {
            anyhow::bail!("Token value cannot be empty.");
        }
        let token_secret = SecretString::new(token_str.clone());
        match validate_token(&platform, token_secret, cache).await {
            Ok(status) => {
                store
                    .set(&platform, &token_str)
                    .map_err(|e| anyhow::anyhow!("Token is valid but failed to store: {e}"))?;
                if let Some(screen) = screen {
                    let mut lines = vec![
                        format!("✓ Authenticated as {}", status.identity),
                        "Token stored securely.".to_string(),
                    ];
                    for warning in &status.warnings {
                        lines.push(format!("warning: {warning}"));
                    }
                    let spec = auth_screen_spec(platform, step, lines, vec![])
                        .severity(ScreenSeverity::Success);
                    screen.render(&spec)?;
                } else {
                    println!("  ✓ Authenticated as {}", status.identity);
                    println!("    Token stored securely.");
                    for warning in &status.warnings {
                        println!("    warning: {warning}");
                    }
                }
            }
            Err(e) => {
                if let Some(screen) = screen {
                    let spec = auth_screen_spec(
                        platform,
                        step,
                        vec!["Authentication failed.".to_string(), format!("{e}")],
                        vec!["Check your token and try again.".to_string()],
                    )
                    .severity(ScreenSeverity::Error);
                    screen.render(&spec)?;
                } else {
                    println!("  ✗ Authentication failed");
                    println!();
                    println!("    {e}");
                }
                return Err(e.into());
            }
        }
        return Ok(());
    }

    // Check if a token already exists
    if let Ok(Some(_)) = store.get(&platform) {
        if let Some(screen) = screen {
            let spec = auth_screen_spec(
                platform,
                step,
                vec![
                    format!("A token is already stored for {}.", platform.display_name()),
                    "Continuing will replace it.".to_string(),
                ],
                vec![],
            )
            .severity(ScreenSeverity::Warning);
            screen.render(&spec)?;
        } else {
            println!();
            println!(
                "  A token is already stored for {}.",
                platform.display_name()
            );
            println!("  Continuing will replace it.");
            println!();
        }
    }

    // In --from-env mode, just check env vars
    if from_env {
        return auth_from_env(&platform, cache).await;
    }

    // Try to detect an existing token from CLI tools before prompting
    if let Some((detected_token, source_name)) = detect_existing_cli_token(&platform) {
        let reuse_spec = auth_screen_spec(
            platform,
            step,
            vec![format!("Found existing token from {source_name} CLI.")],
            vec!["Use detected token now?".to_string()],
        );

        let reuse = if let Some(screen) = screen {
            match screen_confirm(screen, &reuse_spec, "Use this token?", true)? {
                PromptResult {
                    cancelled: true, ..
                } => return Ok(()),
                PromptResult {
                    value: Some(value), ..
                } => value,
                _ => false,
            }
        } else {
            dialoguer::Confirm::new()
                .with_prompt("  Use this token?")
                .default(true)
                .interact()?
        };

        if reuse {
            let token_plain = detected_token.clone();
            let token_secret = SecretString::new(detected_token);

            match validate_token(&platform, token_secret, cache).await {
                Ok(status) => {
                    store.set(&platform, &token_plain).map_err(|e| {
                        anyhow::anyhow!("Token is valid but failed to store in keyring: {e}")
                    })?;
                    if let Some(screen) = screen {
                        let mut lines = vec![
                            format!("✓ Authenticated as {}", status.identity),
                            "Token stored securely.".to_string(),
                        ];
                        for warning in &status.warnings {
                            lines.push(format!("warning: {warning}"));
                        }
                        let spec = auth_screen_spec(platform, step, lines, vec![])
                            .severity(ScreenSeverity::Success);
                        screen.render(&spec)?;
                    } else {
                        println!("  ✓ Authenticated as {}", status.identity);
                        println!("    Token stored securely.");
                        for warning in &status.warnings {
                            println!("    warning: {warning}");
                        }
                        println!();
                    }
                    return Ok(());
                }
                Err(e) => {
                    if let Some(screen) = screen {
                        let spec = auth_screen_spec(
                            platform,
                            step,
                            vec![format!("Validation failed: {e}")],
                            vec!["Falling back to manual token entry.".to_string()],
                        )
                        .severity(ScreenSeverity::Warning);
                        screen.render(&spec)?;
                    } else {
                        println!("  ✗ Validation failed: {e}");
                        println!();
                        println!("  Falling back to manual token entry.");
                        println!();
                    }
                }
            }
        }
    }

    // Prompt for token
    let token = if let Some(screen) = screen {
        match screen_password_masked(screen, &base_spec, "Token", read_token_masked)? {
            PromptResult {
                cancelled: true, ..
            } => return Ok(()),
            PromptResult {
                value: Some(token), ..
            } => token,
            _ => String::new(),
        }
    } else {
        for line in token_help_lines(&platform) {
            println!("  {line}");
        }
        read_token_masked("  Token")?
    };
    let token = token.trim().to_string();

    if token.is_empty() {
        anyhow::bail!("No token provided. Authentication cancelled.");
    }

    // Validate the token
    let token_secret = SecretString::new(token.clone());
    match validate_token(&platform, token_secret, cache).await {
        Ok(status) => {
            store.set(&platform, &token).map_err(|e| {
                anyhow::anyhow!("Token is valid but failed to store in keyring: {e}")
            })?;
            if let Some(screen) = screen {
                let mut lines = vec![
                    format!("✓ Authenticated as {}", status.identity),
                    "Token stored securely.".to_string(),
                ];
                for warning in &status.warnings {
                    lines.push(format!("warning: {warning}"));
                }
                let spec = auth_screen_spec(platform, step, lines, vec![])
                    .severity(ScreenSeverity::Success);
                screen.render(&spec)?;
            } else {
                println!("  ✓ Authenticated as {}", status.identity);
                println!("    Token stored securely.");
                for warning in &status.warnings {
                    println!("    warning: {warning}");
                }
            }
        }
        Err(e) => {
            if let Some(screen) = screen {
                let spec = auth_screen_spec(
                    platform,
                    step,
                    vec!["Authentication failed.".to_string(), format!("{e}")],
                    vec!["Check your token and try again.".to_string()],
                )
                .severity(ScreenSeverity::Error);
                screen.render(&spec)?;
            } else {
                println!("  ✗ Authentication failed");
                println!();
                println!("    {e}");
                println!();
                println!("    → Check your token and try again.");
            }
            return Err(e.into());
        }
    }

    Ok(())
}

fn detect_existing_cli_token(platform: &PlatformKind) -> Option<(String, &'static str)> {
    match platform {
        PlatformKind::GitHub => detect::detect_gh_token().map(|t| (t, "gh")),
        PlatformKind::Railway => detect::detect_railway_token().map(|t| (t, "Railway")),
        PlatformKind::Vercel => detect::detect_vercel_token().map(|t| (t, "Vercel")),
    }
}

async fn auth_from_env(platform: &PlatformKind, cache: &Arc<CacheStore>) -> Result<()> {
    for var_name in platform.env_var_names() {
        if let Ok(token) = std::env::var(var_name) {
            if !token.is_empty() {
                let token_secret = SecretString::new(token);
                match validate_token(platform, token_secret, cache).await {
                    Ok(status) => {
                        println!("  ✓ {var_name} valid ({})", status.identity);
                        for warning in &status.warnings {
                            println!("    warning: {warning}");
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        println!("  ✗ {var_name} invalid — {e}");
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

fn parse_platform_name(name: &str) -> Result<PlatformKind> {
    match name.to_ascii_lowercase().as_str() {
        "github" => Ok(PlatformKind::GitHub),
        "railway" => Ok(PlatformKind::Railway),
        "vercel" => Ok(PlatformKind::Vercel),
        other => anyhow::bail!("Unknown platform: {other}. Use 'github', 'railway', or 'vercel'."),
    }
}

// ── Refresh ───────────────────────────────────────────────────────────────────

async fn auth_refresh(
    platform_name: Option<String>,
    resolver: &TokenResolver,
    store: &Arc<dyn CredentialStore>,
    cache: &Arc<CacheStore>,
    from_env: bool,
    screen: Option<&ScreenSession>,
) -> Result<()> {
    let platforms: Vec<PlatformKind> = if let Some(name) = platform_name {
        vec![parse_platform_name(&name)?]
    } else {
        PlatformKind::ALL.to_vec()
    };

    println!("Refreshing authentication");
    println!("{}", "=".repeat(44));
    println!();

    for platform in &platforms {
        print!("  {:<12}", format!("{}:", platform.display_name()));

        match resolver.resolve_with_source(platform) {
            Some((token, source)) => match validate_token(platform, token, cache).await {
                Ok(status) => {
                    println!("✓ valid via {} ({})", source, status.identity);
                    for warning in &status.warnings {
                        println!("  {:<12}  warning: {}", "", warning);
                    }
                }
                Err(e) => {
                    println!("✗ failed via {} — {}", source, e);
                    println!();
                    auth_platform(*platform, store, cache, from_env, None, screen, None).await?;
                }
            },
            None => {
                println!("not configured");
                println!();
                auth_platform(*platform, store, cache, from_env, None, screen, None).await?;
            }
        }
    }

    println!();
    Ok(())
}

// ── Logout ────────────────────────────────────────────────────────────────────

async fn auth_logout(
    platform_name: Option<String>,
    store: &Arc<dyn CredentialStore>,
    screen: Option<&ScreenSession>,
) -> Result<()> {
    let platforms: Vec<PlatformKind> = if let Some(name) = platform_name {
        vec![parse_platform_name(&name)?]
    } else {
        let items = &["GitHub", "Railway", "Vercel", "All platforms"];
        let selection = if let Some(screen) = screen {
            let spec = ScreenSpec::new("Logout").body_lines([
                "Choose the platform token to remove.",
                "Environment variables are not deleted by Pulsos.",
            ]);
            match screen_select(
                screen,
                &spec,
                "Which platform do you want to logout from?",
                items,
                0,
            )? {
                PromptResult {
                    cancelled: true, ..
                } => return Ok(()),
                PromptResult {
                    value: Some(value), ..
                } => value,
                _ => 0,
            }
        } else {
            dialoguer::Select::new()
                .with_prompt("Which platform do you want to logout from?")
                .items(items)
                .default(0)
                .interact()?
        };

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
            Ok(()) => println!("  Removed {} token.", platform.display_name()),
            Err(e) => println!(
                "  Failed to remove {} token: {}",
                platform.display_name(),
                e
            ),
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

// ── Input helpers ─────────────────────────────────────────────────────────────

/// Interactive password prompt that echoes `*` for each character typed/pasted.
fn read_token_masked(prompt: &str) -> anyhow::Result<String> {
    print!("{}: ", prompt);
    std::io::stdout().flush()?;

    terminal::enable_raw_mode()?;

    let mut input = String::new();
    let result: anyhow::Result<String> = loop {
        match event::read() {
            Ok(Event::Key(key)) => match key.code {
                KeyCode::Enter => {
                    print!("\r\n");
                    let _ = std::io::stdout().flush();
                    break Ok(input);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    print!("\r\n");
                    let _ = std::io::stdout().flush();
                    break Err(anyhow::anyhow!("Authentication cancelled."));
                }
                KeyCode::Backspace => {
                    if !input.is_empty() {
                        input.pop();
                        print!("\x08 \x08");
                        let _ = std::io::stdout().flush();
                    }
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    input.push(c);
                    print!("•");
                    let _ = std::io::stdout().flush();
                }
                _ => {}
            },
            Ok(_) => {}
            Err(e) => {
                let _ = terminal::disable_raw_mode();
                return Err(anyhow::Error::from(e));
            }
        }
    };

    terminal::disable_raw_mode()?;
    result
}

fn token_help_lines(platform: &PlatformKind) -> Vec<String> {
    match platform {
        PlatformKind::GitHub => vec![
            "Create a personal access token:".to_string(),
            "https://github.com/settings/tokens".to_string(),
            "Required scopes: repo, read:org".to_string(),
        ],
        PlatformKind::Railway => vec![
            "Create an Account token:".to_string(),
            "https://railway.com/account/tokens".to_string(),
            "Token type: Account (NOT Workspace or Project tokens)".to_string(),
        ],
        PlatformKind::Vercel => vec![
            "Create a token:".to_string(),
            "https://vercel.com/account/tokens".to_string(),
        ],
    }
}

fn auth_screen_spec(
    platform: PlatformKind,
    step: Option<(usize, usize)>,
    body_lines: Vec<String>,
    hints: Vec<String>,
) -> ScreenSpec {
    let mut spec = ScreenSpec::new(platform_display_name(platform))
        .subtitle("Authentication")
        .body_lines(body_lines)
        .hints(hints);
    if let Some((index, total)) = step {
        spec = spec.step(index, total);
    }
    spec
}
