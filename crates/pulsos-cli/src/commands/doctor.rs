use anyhow::Result;
use clap::Args;
use pulsos_core::auth::credential_store::KeyringStore;
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::validate::validate_token;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::load_config;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Args)]
pub struct DoctorArgs {}

pub async fn execute(_args: DoctorArgs, config_path: Option<&Path>) -> Result<()> {
    println!("Pulsos Doctor v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", "=".repeat(44));
    println!();

    // Config check
    let config = {
        print!("  Config file:  ");
        match load_config(config_path) {
            Ok(c) => {
                println!("found");
                Some(c)
            }
            Err(e) => {
                println!("MISSING - {e}");
                println!();
                println!("  Run `pulsos repos sync` to create a configuration.");
                None
            }
        }
    };

    let cache = Arc::new(CacheStore::open_default()?);

    // Cache check
    print!("  Cache:        ");
    println!("{} entries", cache.len());
    println!();

    // Build token resolver
    let store = Arc::new(KeyringStore::new());
    let detection_config = config
        .as_ref()
        .map(|c| c.auth.token_detection.clone())
        .unwrap_or_default();
    let resolver = TokenResolver::new(store, detection_config);

    // Platform auth checks via TokenResolver
    println!("  Platforms");

    for platform in &PlatformKind::ALL {
        print!("    {:<12}", format!("{}:", platform.display_name()));

        match resolver.resolve_with_source(platform) {
            Some((token, source)) => match validate_token(platform, token, &cache).await {
                Ok(status) => {
                    println!("OK via {} ({})", source, status.identity);
                    for warning in &status.warnings {
                        println!("    {:<12}  warning: {}", "", warning);
                    }
                }
                Err(e) => {
                    println!("FAIL via {} - {}", source, e);
                }
            },
            None => {
                println!("no token found");
            }
        }
    }

    println!();
    println!("  Done.");

    Ok(())
}
