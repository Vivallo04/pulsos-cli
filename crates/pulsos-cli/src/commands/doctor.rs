//! The `pulsos doctor` command — comprehensive diagnostics.
//!
//! Checks system info, authentication, API connectivity, rate limits,
//! tracked resources, correlations, cache, and optional CLI detection.

use super::doctor_fmt::{
    count_issues, format_bytes, print_check, print_section, print_summary, CheckResult, CheckStatus,
};
use anyhow::Result;
use clap::Args;
use pulsos_core::auth::credential_store::KeyringStore;
use pulsos_core::auth::detect;
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::load_config;
use pulsos_core::config::types::PulsosConfig;
use pulsos_core::health::{check_all_platforms_health, PlatformHealthReport, PlatformHealthState};
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::PlatformAdapter;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Args)]
pub struct DoctorArgs {}

pub async fn execute(_args: DoctorArgs, config_path: Option<&Path>) -> Result<()> {
    println!("Pulsos Doctor v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", "═".repeat(50));
    println!();

    let config = load_config(config_path).ok();
    let cache = Arc::new(CacheStore::open_default()?);
    let store = Arc::new(KeyringStore::new());
    let detection_config = config
        .as_ref()
        .map(|c| c.auth.token_detection.clone())
        .unwrap_or_default();
    let resolver = TokenResolver::new(store, detection_config);

    let mut total_warnings = 0usize;
    let mut total_errors = 0usize;
    let mut suggestions: Vec<String> = Vec::new();
    let health_reports =
        check_all_platforms_health(&config.clone().unwrap_or_default(), &resolver, &cache).await;

    // Section 1: System
    let results = check_system();
    print_section("System");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
    }
    println!();

    // Section 2: Authentication
    let results = check_auth(&health_reports);
    print_section("Authentication");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
        collect_auth_suggestions(r, &mut suggestions);
    }
    println!();

    // Section 3: API Connectivity
    let results = check_connectivity(&health_reports);
    print_section("API Connectivity");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
    }
    println!();

    // Section 4: Rate Limits
    let results = check_rate_limits(&resolver, &cache).await;
    print_section("Rate Limits");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
    }
    println!();

    // Section 5: Tracked Resources
    let results = check_tracked_resources(&config);
    print_section("Tracked Resources");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
    }
    println!();

    // Section 6: Correlations
    let results = check_correlations(&config);
    print_section("Correlations");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
        collect_correlation_suggestions(r, &mut suggestions);
    }
    println!();

    // Section 7: Cache
    let results = check_cache(&cache);
    print_section("Cache");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
    }
    println!();

    // Section 8: Optional CLI Detection
    let results = check_cli_tools();
    print_section("Optional CLI Detection");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
    }
    println!();

    // Section 9: Daemon
    let results = check_daemon().await;
    print_section("Daemon");
    for r in &results {
        print_check(r);
        count_issues(r, &mut total_warnings, &mut total_errors);
    }
    println!();

    // Section 10: Summary
    print_summary(total_warnings, total_errors, &suggestions);

    Ok(())
}

// ── Section 1: System ──

fn check_system() -> Vec<CheckResult> {
    let mut results = Vec::new();

    // OS + arch
    let os_info = format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH);
    results.push(CheckResult::ok("OS", os_info));

    // Shell
    let shell = std::env::var("SHELL")
        .or_else(|_| std::env::var("COMSPEC"))
        .unwrap_or_else(|_| "unknown".to_string());
    let shell_name = std::path::Path::new(&shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&shell)
        .to_string();
    results.push(CheckResult::ok("Shell", shell_name.to_string()));

    // Terminal
    let terminal = std::env::var("TERM_PROGRAM")
        .or_else(|_| std::env::var("TERM"))
        .unwrap_or_else(|_| "unknown".to_string());
    results.push(CheckResult::ok("Terminal", terminal));

    results
}

// ── Section 2: Authentication ──

fn check_auth(reports: &[PlatformHealthReport]) -> Vec<CheckResult> {
    let mut results = Vec::new();

    for report in reports {
        let mut result = match report.state {
            PlatformHealthState::Ready => {
                CheckResult::ok(report.platform.display_name(), report.state.label())
            }
            PlatformHealthState::NoToken => {
                CheckResult::warning(report.platform.display_name(), report.state.label())
            }
            PlatformHealthState::AccessOrConfigIncomplete => {
                CheckResult::warning(report.platform.display_name(), report.state.label())
            }
            PlatformHealthState::InvalidToken => {
                CheckResult::error(report.platform.display_name(), report.state.label())
            }
            PlatformHealthState::ConnectivityError => {
                CheckResult::error(report.platform.display_name(), report.state.label())
            }
        };

        result = result.with_detail(format!("{} Next: {}", report.reason, report.next_action));
        results.push(result);
    }

    results
}

fn collect_auth_suggestions(result: &CheckResult, suggestions: &mut Vec<String>) {
    if result.value.contains("No Token") {
        suggestions.push(format!(
            "Run `pulsos auth {}` to authenticate.",
            result.label.to_lowercase()
        ));
    }
    if result.value.contains("Invalid Token") {
        suggestions.push(format!(
            "Run `pulsos auth {}` to refresh token.",
            result.label.to_lowercase()
        ));
    }
}

// ── Section 3: API Connectivity ──

fn check_connectivity(reports: &[PlatformHealthReport]) -> Vec<CheckResult> {
    let mut results = Vec::new();

    let endpoints = [
        (PlatformKind::GitHub, "api.github.com"),
        (PlatformKind::Railway, "backboard.railway.com"),
        (PlatformKind::Vercel, "api.vercel.com"),
    ];

    for (platform, host) in &endpoints {
        let Some(report) = reports.iter().find(|r| r.platform == *platform) else {
            results.push(CheckResult::skipped(*host, "no health report"));
            continue;
        };

        match report.state {
            PlatformHealthState::NoToken => {
                results.push(CheckResult::skipped(*host, "skipped (no token)"));
            }
            PlatformHealthState::ConnectivityError => {
                results.push(CheckResult::error(
                    *host,
                    format!("unreachable — {}", report.reason),
                ));
            }
            _ => {
                results.push(CheckResult::ok(
                    *host,
                    "reachable (validated auth endpoint)",
                ));
            }
        }
    }

    results
}

// ── Section 4: Rate Limits ──

async fn check_rate_limits(resolver: &TokenResolver, cache: &Arc<CacheStore>) -> Vec<CheckResult> {
    let mut results = Vec::new();

    // GitHub has visible rate limits
    if let Some(token) = resolver.resolve(&PlatformKind::GitHub) {
        match GitHubClient::new(token, cache.clone()) {
            Ok(client) => match client.rate_limit_status().await {
                Ok(info) => {
                    let value = format!(
                        "{} / {} remaining (resets {})",
                        info.remaining,
                        info.limit,
                        info.resets_at.format("%H:%M"),
                    );
                    if info.remaining < info.limit / 10 {
                        results.push(CheckResult::warning("GitHub", value));
                    } else {
                        results.push(CheckResult::ok("GitHub", value));
                    }
                }
                Err(e) => {
                    results.push(CheckResult::warning(
                        "GitHub",
                        format!("Unable to fetch rate limits: {e}"),
                    ));
                }
            },
            Err(e) => results.push(CheckResult::warning(
                "GitHub",
                format!("Unable to initialize client: {e}"),
            )),
        }
    } else {
        results.push(CheckResult::skipped("GitHub", "skipped (no token)"));
    }

    // Railway and Vercel don't expose public rate limits
    results.push(CheckResult::ok("Railway", "OK"));
    results.push(CheckResult::ok("Vercel", "OK"));

    results
}

// ── Section 5: Tracked Resources ──

fn check_tracked_resources(config: &Option<PulsosConfig>) -> Vec<CheckResult> {
    let mut results = Vec::new();

    let config = match config {
        Some(c) => c,
        None => {
            results.push(CheckResult::warning(
                "Config",
                "no config — run `pulsos repos sync`",
            ));
            return results;
        }
    };

    if config.correlations.is_empty() {
        results.push(CheckResult::warning(
            "Resources",
            "no tracked resources — run `pulsos repos sync`",
        ));
        return results;
    }

    // Count by platform, grouped by org/workspace/team
    let mut github_groups: std::collections::HashMap<String, usize> = Default::default();
    let mut railway_groups: std::collections::HashMap<String, usize> = Default::default();
    let mut vercel_groups: std::collections::HashMap<String, usize> = Default::default();

    for corr in &config.correlations {
        if corr.github_repo.is_some() {
            let org = corr
                .github_repo
                .as_ref()
                .and_then(|r| r.split('/').next())
                .unwrap_or("personal")
                .to_string();
            *github_groups.entry(org).or_default() += 1;
        }
        if corr.railway_project.is_some() {
            let ws = corr
                .railway_workspace
                .clone()
                .unwrap_or_else(|| "default".to_string());
            *railway_groups.entry(ws).or_default() += 1;
        }
        if corr.vercel_project.is_some() {
            let team = corr
                .vercel_team
                .clone()
                .unwrap_or_else(|| "personal".to_string());
            *vercel_groups.entry(team).or_default() += 1;
        }
    }

    if !github_groups.is_empty() {
        let total: usize = github_groups.values().sum();
        let detail = format_groups(&github_groups);
        results.push(CheckResult::ok(
            "GitHub repos",
            format!("{total} repos ({detail})"),
        ));
    }

    if !railway_groups.is_empty() {
        let total: usize = railway_groups.values().sum();
        let detail = format_groups(&railway_groups);
        let noun = if total == 1 { "project" } else { "projects" };
        results.push(CheckResult::ok(
            "Railway",
            format!("{total} {noun} ({detail})"),
        ));
    }

    if !vercel_groups.is_empty() {
        let total: usize = vercel_groups.values().sum();
        let detail = format_groups(&vercel_groups);
        let noun = if total == 1 { "project" } else { "projects" };
        results.push(CheckResult::ok(
            "Vercel",
            format!("{total} {noun} ({detail})"),
        ));
    }

    results
}

fn format_groups(groups: &std::collections::HashMap<String, usize>) -> String {
    let mut parts: Vec<String> = groups
        .iter()
        .map(|(name, count)| format!("{name}: {count}"))
        .collect();
    parts.sort();
    parts.join(", ")
}

// ── Section 6: Correlations ──

fn check_correlations(config: &Option<PulsosConfig>) -> Vec<CheckResult> {
    let mut results = Vec::new();

    let config = match config {
        Some(c) => c,
        None => {
            results.push(CheckResult::skipped("Correlations", "no config"));
            return results;
        }
    };

    if config.correlations.is_empty() {
        results.push(CheckResult::skipped("Correlations", "none configured"));
        return results;
    }

    let mut matched = 0usize;
    let mut standalone = 0usize;
    let mut standalone_names: Vec<String> = Vec::new();

    for corr in &config.correlations {
        let platform_count = [
            corr.github_repo.is_some(),
            corr.railway_project.is_some(),
            corr.vercel_project.is_some(),
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        if platform_count >= 2 {
            matched += 1;
        } else {
            standalone += 1;
            standalone_names.push(corr.name.clone());
        }
    }

    results.push(CheckResult::ok(
        "Matched",
        format!("{matched} project groups across 2+ platforms"),
    ));

    if standalone > 0 {
        let names = standalone_names.join(", ");
        results.push(
            CheckResult::warning("Standalone", format!("{standalone} projects ({names})"))
                .with_detail("Run `pulsos repos sync` to link platforms"),
        );
    } else {
        results.push(CheckResult::ok("Standalone", "none"));
    }

    results
}

fn collect_correlation_suggestions(result: &CheckResult, suggestions: &mut Vec<String>) {
    if result.status == CheckStatus::Warning && result.label == "Standalone" {
        suggestions.push("Run `pulsos repos sync` to link unmatched projects.".into());
    }
}

// ── Section 7: Cache ──

fn check_cache(cache: &Arc<CacheStore>) -> Vec<CheckResult> {
    let mut results = Vec::new();

    // Location
    let cache_dir = dirs::cache_dir()
        .map(|d| d.join("pulsos"))
        .unwrap_or_default();
    results.push(CheckResult::ok("Location", cache_dir.display().to_string()));

    // Size
    let size = cache.disk_size();
    let entry_count = cache.len();
    results.push(CheckResult::ok(
        "Size",
        format!("{} ({entry_count} entries)", format_bytes(size)),
    ));

    // Oldest entry
    match cache.oldest_entry_age() {
        Some(age) => {
            let age_str = if age.as_secs() < 60 {
                "just now".to_string()
            } else if age.as_secs() < 3600 {
                format!("{} minutes ago", age.as_secs() / 60)
            } else {
                format!("{} hours ago", age.as_secs() / 3600)
            };
            results.push(CheckResult::ok("Oldest entry", age_str));
        }
        None => {
            results.push(CheckResult::ok("Oldest entry", "empty cache"));
        }
    }

    results
}

// ── Section 8: Optional CLI Detection ──

fn check_cli_tools() -> Vec<CheckResult> {
    let mut results = Vec::new();

    // gh CLI
    let gh_result = detect_cli_version("gh");
    match gh_result {
        Some(version) => {
            let token_reusable = detect::detect_gh_token().is_some();
            let token_info = if token_reusable {
                "token detected and reusable"
            } else {
                "no token detected"
            };
            results.push(CheckResult::ok(
                "gh CLI",
                format!("{version} ({token_info})"),
            ));
        }
        None => {
            results.push(CheckResult::skipped(
                "gh CLI",
                "not installed (not required)",
            ));
        }
    }

    // railway CLI
    let railway_result = detect_cli_version("railway");
    match railway_result {
        Some(version) => {
            let token_reusable = detect::detect_railway_token().is_some();
            let token_info = if token_reusable {
                "token detected and reusable"
            } else {
                "no token detected"
            };
            results.push(CheckResult::ok(
                "railway CLI",
                format!("{version} ({token_info})"),
            ));
        }
        None => {
            results.push(CheckResult::skipped(
                "railway CLI",
                "not installed (not required)",
            ));
        }
    }

    // vercel CLI
    let vercel_result = detect_cli_version("vercel");
    match vercel_result {
        Some(version) => {
            let token_reusable = detect::detect_vercel_token().is_some();
            let token_info = if token_reusable {
                "token detected and reusable"
            } else {
                "no token detected"
            };
            results.push(CheckResult::ok(
                "vercel CLI",
                format!("{version} ({token_info})"),
            ));
        }
        None => {
            results.push(CheckResult::skipped(
                "vercel CLI",
                "not installed (not required)",
            ));
        }
    }

    results
}

// ── Section 9: Daemon ──

async fn check_daemon() -> Vec<CheckResult> {
    let mut results = Vec::new();

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("pulsos");

    let port_path = config_dir.join("daemon.port");
    let token_path = config_dir.join("daemon.token");

    // Check whether a port file exists at all.
    let port: Option<u16> = std::fs::read_to_string(&port_path)
        .ok()
        .and_then(|s| s.trim().parse().ok());

    let Some(port) = port else {
        results.push(CheckResult::ok("Status", "not running"));
        return results;
    };

    // Port file exists — check if daemon actually responds.
    let url = format!("http://127.0.0.1:{port}/health");
    let alive = reqwest::get(&url)
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    if !alive {
        results.push(
            CheckResult::warning(
                "Status",
                format!("port file exists (port {port}) but /health did not respond"),
            )
            .with_detail(
                "Run `pulsos daemon stop` to clean up stale files, then `pulsos daemon start`",
            ),
        );
        return results;
    }

    results.push(CheckResult::ok("Status", format!("running on port {port}")));

    // Check token file exists.
    if !token_path.exists() {
        results.push(
            CheckResult::warning("Token file", "daemon.token missing")
                .with_detail("The SSE stream cannot be authenticated. Restart the daemon."),
        );
        return results;
    }

    results.push(CheckResult::ok(
        "Token file",
        token_path.display().to_string(),
    ));

    // On Unix, verify mode is 0o600.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(&token_path) {
            Ok(meta) => {
                let mode = meta.permissions().mode() & 0o777;
                if mode == 0o600 {
                    results.push(CheckResult::ok("Token permissions", "0600 (secure)"));
                } else {
                    results.push(
                        CheckResult::warning(
                            "Token permissions",
                            format!("{:04o} (expected 0600)", mode),
                        )
                        .with_detail("Fix with: chmod 600 ~/.config/pulsos/daemon.token"),
                    );
                }
            }
            Err(e) => {
                results.push(CheckResult::warning(
                    "Token permissions",
                    format!("could not read metadata: {e}"),
                ));
            }
        }
    }

    results
}

/// Try to get a CLI tool's version via `tool --version`, with a 2-second timeout.
fn detect_cli_version(tool: &str) -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let tool_owned = tool.to_string();
    std::thread::spawn(move || {
        let result = std::process::Command::new(&tool_owned)
            .arg("--version")
            .output()
            .ok();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Some(o)) => o,
        _ => return None,
    };

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Extract just the version number from output like "gh version 2.45.0" or "railway 3.9.0"
    let version = stdout.lines().next().unwrap_or("").trim().to_string();

    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_system_returns_results() {
        let results = check_system();
        assert!(results.len() >= 3);
        assert!(results.iter().all(|r| r.status == CheckStatus::Ok));
        assert_eq!(results[0].label, "OS");
        assert_eq!(results[1].label, "Shell");
        assert_eq!(results[2].label, "Terminal");
    }

    #[test]
    fn check_tracked_resources_no_config() {
        let results = check_tracked_resources(&None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Warning);
    }

    #[test]
    fn check_tracked_resources_with_correlations() {
        let config = PulsosConfig {
            correlations: vec![
                pulsos_core::config::types::CorrelationConfig {
                    name: "proj-a".into(),
                    github_repo: Some("myorg/proj-a".into()),
                    railway_project: Some("proj-a-api".into()),
                    railway_workspace: Some("lambda-prod".into()),
                    railway_environment: None,
                    vercel_project: Some("proj-a-web".into()),
                    vercel_team: Some("Lambda".into()),
                    branch_mapping: Default::default(),
                },
                pulsos_core::config::types::CorrelationConfig {
                    name: "proj-b".into(),
                    github_repo: Some("myorg/proj-b".into()),
                    railway_project: None,
                    railway_workspace: None,
                    railway_environment: None,
                    vercel_project: None,
                    vercel_team: None,
                    branch_mapping: Default::default(),
                },
            ],
            ..Default::default()
        };

        let results = check_tracked_resources(&Some(config));
        // Should have GitHub (2 repos) + Railway (1 project) + Vercel (1 project) = 3 results
        assert_eq!(results.len(), 3);
        assert!(results[0].value.contains("2 repos"));
        assert!(results[1].value.contains("1 project"));
        assert!(results[2].value.contains("1 project"));
    }

    #[test]
    fn check_correlations_no_config() {
        let results = check_correlations(&None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Skipped);
    }

    #[test]
    fn check_correlations_all_matched() {
        let config = PulsosConfig {
            correlations: vec![pulsos_core::config::types::CorrelationConfig {
                name: "proj-a".into(),
                github_repo: Some("myorg/proj-a".into()),
                railway_project: Some("proj-a-api".into()),
                railway_workspace: None,
                railway_environment: None,
                vercel_project: None,
                vercel_team: None,
                branch_mapping: Default::default(),
            }],
            ..Default::default()
        };

        let results = check_correlations(&Some(config));
        assert_eq!(results.len(), 2); // Matched + Standalone
        assert!(results[0].value.contains("1 project groups"));
        assert_eq!(results[1].value, "none");
    }

    #[test]
    fn check_correlations_with_standalone() {
        let config = PulsosConfig {
            correlations: vec![
                pulsos_core::config::types::CorrelationConfig {
                    name: "proj-a".into(),
                    github_repo: Some("myorg/proj-a".into()),
                    railway_project: Some("proj-a-api".into()),
                    railway_workspace: None,
                    railway_environment: None,
                    vercel_project: None,
                    vercel_team: None,
                    branch_mapping: Default::default(),
                },
                pulsos_core::config::types::CorrelationConfig {
                    name: "solo-proj".into(),
                    github_repo: Some("myorg/solo".into()),
                    railway_project: None,
                    railway_workspace: None,
                    railway_environment: None,
                    vercel_project: None,
                    vercel_team: None,
                    branch_mapping: Default::default(),
                },
            ],
            ..Default::default()
        };

        let results = check_correlations(&Some(config));
        assert_eq!(results.len(), 2);
        assert!(results[0].value.contains("1 project groups"));
        assert_eq!(results[1].status, CheckStatus::Warning);
        assert!(results[1].value.contains("solo-proj"));
    }

    #[test]
    fn check_cli_tools_returns_results() {
        let results = check_cli_tools();
        // Should always return 3 results (one per CLI tool)
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].label, "gh CLI");
        assert_eq!(results[1].label, "railway CLI");
        assert_eq!(results[2].label, "vercel CLI");
    }

    #[test]
    fn check_cache_returns_results() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheStore::open(dir.path()).unwrap();
        let cache = Arc::new(cache);

        let results = check_cache(&cache);
        assert!(results.len() >= 3);
        assert_eq!(results[0].label, "Location");
        assert_eq!(results[1].label, "Size");
        assert_eq!(results[2].label, "Oldest entry");
        assert!(results[2].value.contains("empty cache"));
    }
}
