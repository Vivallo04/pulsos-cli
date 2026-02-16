use anyhow::Result;
use clap::Args;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::load_config;
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::PlatformAdapter;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Args)]
pub struct DoctorArgs {}

/// Resolve a token from environment variable.
fn env_token(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|t| !t.is_empty())
}

pub async fn execute(_args: DoctorArgs, config_path: Option<&Path>) -> Result<()> {
    println!("Pulsos Doctor v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", "=".repeat(44));
    println!();

    // Config check
    print!("  Config file:  ");
    match load_config(config_path) {
        Ok(_) => println!("found"),
        Err(e) => {
            println!("MISSING - {e}");
            println!();
            println!("  Run `pulsos repos sync` to create a configuration.");
        }
    }

    let cache = Arc::new(CacheStore::open_default()?);

    // Cache check
    print!("  Cache:        ");
    println!("{} entries", cache.len());
    println!();

    // Platform auth checks via env tokens
    println!("  Platforms");

    // GitHub
    print!("    GitHub:     ");
    if let Some(token) = env_token("GITHUB_TOKEN") {
        let client = GitHubClient::new(token, cache.clone());
        match client.validate_auth().await {
            Ok(status) => println!("OK ({})", status.identity),
            Err(e) => println!("FAIL - {e}"),
        }
    } else {
        println!("no GITHUB_TOKEN set");
    }

    // Railway
    print!("    Railway:    ");
    if let Some(token) = env_token("RAILWAY_TOKEN") {
        let client = RailwayClient::new(token, cache.clone());
        match client.validate_auth().await {
            Ok(status) => println!("OK ({})", status.identity),
            Err(e) => println!("FAIL - {e}"),
        }
    } else {
        println!("no RAILWAY_TOKEN set");
    }

    // Vercel
    print!("    Vercel:     ");
    if let Some(token) = env_token("VERCEL_TOKEN") {
        let client = VercelClient::new(token, cache.clone());
        match client.validate_auth().await {
            Ok(status) => println!("OK ({})", status.identity),
            Err(e) => println!("FAIL - {e}"),
        }
    } else {
        println!("no VERCEL_TOKEN set");
    }

    println!();
    println!("  Done.");

    Ok(())
}
