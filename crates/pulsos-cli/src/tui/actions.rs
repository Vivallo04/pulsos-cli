use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use pulsos_core::auth::credential_store::{CredentialStore, FallbackStore};
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::validate::validate_token;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::types::{PulsosConfig, TuiConfig};
use pulsos_core::config::{load_config, save_config};
use pulsos_core::platform::github::client::GitHubClient;
use pulsos_core::platform::railway::client::RailwayClient;
use pulsos_core::platform::vercel::client::VercelClient;
use pulsos_core::platform::PlatformAdapter;
use pulsos_core::sync::correlate::{build_correlations, candidate_to_config, DiscoveryResults};
use pulsos_core::sync::merge::{merge_correlations, populate_platform_sections};

use super::settings_flow::DiscoveryPayload;

#[derive(Clone)]
pub enum ActionRequest {
    ValidateAndStoreToken {
        platform: PlatformKind,
        token: String,
    },
    RemoveToken {
        platform: PlatformKind,
    },
    ValidatePlatform {
        platform: PlatformKind,
    },
    Discover {
        platforms: Vec<PlatformKind>,
    },
    BuildCorrelationPreview {
        discovery: DiscoveryPayload,
    },
    ApplyCorrelations {
        discovery: DiscoveryPayload,
    },
    StartDaemon,
    StopDaemon,
    SaveTuiConfig {
        tui: Box<TuiConfig>,
    },
}

impl std::fmt::Debug for ActionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidateAndStoreToken { platform, .. } => f
                .debug_struct("ValidateAndStoreToken")
                .field("platform", platform)
                .field("token", &"[REDACTED]")
                .finish(),
            Self::RemoveToken { platform } => f
                .debug_struct("RemoveToken")
                .field("platform", platform)
                .finish(),
            Self::ValidatePlatform { platform } => f
                .debug_struct("ValidatePlatform")
                .field("platform", platform)
                .finish(),
            Self::Discover { platforms } => f
                .debug_struct("Discover")
                .field("platforms", platforms)
                .finish(),
            Self::BuildCorrelationPreview { .. } => f
                .debug_struct("BuildCorrelationPreview")
                .finish_non_exhaustive(),
            Self::ApplyCorrelations { .. } => {
                f.debug_struct("ApplyCorrelations").finish_non_exhaustive()
            }
            Self::StartDaemon => f.debug_struct("StartDaemon").finish(),
            Self::StopDaemon => f.debug_struct("StopDaemon").finish(),
            Self::SaveTuiConfig { .. } => {
                f.debug_struct("SaveTuiConfig").finish_non_exhaustive()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ActionResult {
    TokenStored {
        platform: PlatformKind,
        identity: String,
        warnings: Vec<String>,
    },
    TokenRemoved {
        platform: PlatformKind,
    },
    PlatformValidated {
        platform: PlatformKind,
        identity: String,
        warnings: Vec<String>,
    },
    DiscoveryCompleted {
        payload: DiscoveryPayload,
    },
    CorrelationPreview {
        lines: Vec<String>,
    },
    CorrelationsApplied {
        added: usize,
        updated: usize,
        total: usize,
        config: Box<PulsosConfig>,
    },
    Error {
        context: String,
        message: String,
    },
    DaemonStarted,
    DaemonStopped,
    DaemonAlreadyRunning,
    DaemonNotRunning,
    TuiConfigSaved,
}

pub async fn run_worker(
    config_path: Option<PathBuf>,
    mut rx: tokio::sync::mpsc::Receiver<ActionRequest>,
    tx: tokio::sync::mpsc::Sender<ActionResult>,
    cache: Arc<CacheStore>,
) {
    let store: Arc<dyn CredentialStore> = match FallbackStore::new() {
        Ok(store) => Arc::new(store),
        Err(err) => {
            let _ = tx
                .send(ActionResult::Error {
                    context: "init".to_string(),
                    message: err.user_message(),
                })
                .await;
            return;
        }
    };

    while let Some(request) = rx.recv().await {
        let result = handle_request(&request, &config_path, &store, &cache).await;
        if tx.send(result).await.is_err() {
            return;
        }
    }
}

async fn handle_request(
    request: &ActionRequest,
    config_path: &Option<PathBuf>,
    store: &Arc<dyn CredentialStore>,
    cache: &Arc<CacheStore>,
) -> ActionResult {
    match request {
        ActionRequest::ValidateAndStoreToken { platform, token } => {
            let token = token.trim().to_string();
            if token.is_empty() {
                return ActionResult::Error {
                    context: format!("{} token", platform.display_name()),
                    message: "Token cannot be empty.".to_string(),
                };
            }

            match validate_token(platform, secrecy::SecretString::new(token.clone()), cache).await {
                Ok(status) => match store.set(platform, &token) {
                    Ok(()) => ActionResult::TokenStored {
                        platform: *platform,
                        identity: status.identity,
                        warnings: status.warnings,
                    },
                    Err(err) => ActionResult::Error {
                        context: format!("store {}", platform.display_name()),
                        message: err.user_message(),
                    },
                },
                Err(err) => ActionResult::Error {
                    context: format!("validate {}", platform.display_name()),
                    message: err.user_message(),
                },
            }
        }
        ActionRequest::RemoveToken { platform } => match store.delete(platform) {
            Ok(()) => ActionResult::TokenRemoved {
                platform: *platform,
            },
            Err(err) => ActionResult::Error {
                context: format!("remove {}", platform.display_name()),
                message: err.user_message(),
            },
        },
        ActionRequest::ValidatePlatform { platform } => {
            let config = load_or_default(config_path.as_deref());
            let resolver = TokenResolver::new(store.clone(), config.auth.token_detection.clone());
            let Some(token) = resolver.resolve(platform) else {
                return ActionResult::Error {
                    context: format!("validate {}", platform.display_name()),
                    message: "No token is configured for this platform.".to_string(),
                };
            };
            match validate_token(platform, token, cache).await {
                Ok(status) => ActionResult::PlatformValidated {
                    platform: *platform,
                    identity: status.identity,
                    warnings: status.warnings,
                },
                Err(err) => ActionResult::Error {
                    context: format!("validate {}", platform.display_name()),
                    message: err.user_message(),
                },
            }
        }
        ActionRequest::Discover { platforms } => {
            let config = load_or_default(config_path.as_deref());
            let resolver = TokenResolver::new(store.clone(), config.auth.token_detection.clone());
            let mut payload = DiscoveryPayload::default();

            for platform in platforms {
                let Some(token) = resolver.resolve(platform) else {
                    payload.warnings.push(format!(
                        "{} skipped: no token configured.",
                        platform.display_name()
                    ));
                    continue;
                };

                match platform {
                    PlatformKind::GitHub => match GitHubClient::new(token, cache.clone()) {
                        Ok(client) => match client.discover().await {
                            Ok(resources) => {
                                payload.github = resources
                                    .into_iter()
                                    .filter(|r| !r.archived && !r.disabled)
                                    .collect();
                            }
                            Err(err) => payload
                                .warnings
                                .push(format!("GitHub discovery failed: {}", err.user_message())),
                        },
                        Err(err) => payload
                            .warnings
                            .push(format!("GitHub init failed: {}", err.user_message())),
                    },
                    PlatformKind::Railway => match RailwayClient::new(token, cache.clone()) {
                        Ok(client) => match client.discover().await {
                            Ok(resources) => {
                                payload.railway = resources
                                    .into_iter()
                                    .filter(|r| !r.archived && !r.disabled)
                                    .collect();
                            }
                            Err(err) => payload
                                .warnings
                                .push(format!("Railway discovery failed: {}", err.user_message())),
                        },
                        Err(err) => payload
                            .warnings
                            .push(format!("Railway init failed: {}", err.user_message())),
                    },
                    PlatformKind::Vercel => match VercelClient::new(token, cache.clone()) {
                        Ok(client) => match client.discover_with_links().await {
                            Ok(resources) => {
                                payload.vercel = resources
                                    .into_iter()
                                    .filter(|(r, _)| !r.archived && !r.disabled)
                                    .collect();
                            }
                            Err(err) => payload
                                .warnings
                                .push(format!("Vercel discovery failed: {}", err.user_message())),
                        },
                        Err(err) => payload
                            .warnings
                            .push(format!("Vercel init failed: {}", err.user_message())),
                    },
                }
            }

            ActionResult::DiscoveryCompleted { payload }
        }
        ActionRequest::BuildCorrelationPreview { discovery } => {
            let candidates = build_candidates(discovery);
            let mut lines = Vec::new();
            for candidate in candidates {
                lines.push(format!("{} ({:?})", candidate.name, candidate.confidence));
            }
            if lines.is_empty() {
                lines.push(
                    "No correlation candidates produced from selected resources.".to_string(),
                );
            }
            ActionResult::CorrelationPreview { lines }
        }
        ActionRequest::ApplyCorrelations { discovery } => {
            let candidates = build_candidates(discovery);
            if candidates.is_empty() {
                return ActionResult::Error {
                    context: "apply correlations".to_string(),
                    message: "No correlation candidates to save.".to_string(),
                };
            }

            let existing = load_or_default(config_path.as_deref());
            let new_correlations = candidates.iter().map(candidate_to_config).collect();
            let (mut merged, added, updated) = merge_correlations(&existing, new_correlations);
            populate_platform_sections(&mut merged);
            if let Err(err) = save_config(&merged, config_path.as_deref()) {
                return ActionResult::Error {
                    context: "save config".to_string(),
                    message: err.user_message(),
                };
            }

            ActionResult::CorrelationsApplied {
                added,
                updated,
                total: merged.correlations.len(),
                config: Box::new(merged),
            }
        }
        ActionRequest::StartDaemon => {
            // Check if already running by reading pid file.
            let pid_running = {
                let pid_opt = dirs::config_dir()
                    .map(|d| d.join("pulsos").join("daemon.pid"))
                    .and_then(|p| std::fs::read_to_string(&p).ok())
                    .and_then(|s| s.trim().parse::<u32>().ok());
                match pid_opt {
                    None => false,
                    Some(pid) => {
                        #[cfg(unix)]
                        {
                            verify_pid_is_daemon(pid)
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = pid;
                            false
                        }
                    }
                }
            };

            if pid_running {
                return ActionResult::DaemonAlreadyRunning;
            }

            let exe = match std::env::current_exe() {
                Ok(p) => p,
                Err(e) => {
                    return ActionResult::Error {
                        context: "start daemon".to_string(),
                        message: e.to_string(),
                    }
                }
            };
            match std::process::Command::new(&exe)
                .args(["daemon", "start"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(_) => ActionResult::DaemonStarted,
                Err(e) => ActionResult::Error {
                    context: "start daemon".to_string(),
                    message: e.to_string(),
                },
            }
        }
        ActionRequest::StopDaemon => {
            let pid_path = match dirs::config_dir() {
                Some(d) => d.join("pulsos").join("daemon.pid"),
                None => {
                    return ActionResult::DaemonNotRunning;
                }
            };
            let pid = match std::fs::read_to_string(&pid_path)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
            {
                Some(p) => p,
                None => return ActionResult::DaemonNotRunning,
            };
            #[cfg(unix)]
            {
                // Verify this PID belongs to our daemon before sending SIGTERM
                // to avoid terminating an unrelated process that reused the PID.
                if !verify_pid_is_daemon(pid) {
                    let _ = std::fs::remove_file(&pid_path);
                    return ActionResult::DaemonNotRunning;
                }
                let success = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if success {
                    let _ = std::fs::remove_file(&pid_path);
                    ActionResult::DaemonStopped
                } else {
                    ActionResult::DaemonNotRunning
                }
            }
            #[cfg(not(unix))]
            {
                let _ = pid;
                ActionResult::DaemonNotRunning
            }
        }
        ActionRequest::SaveTuiConfig { tui } => {
            let mut config = load_or_default(config_path.as_deref());
            config.tui = *tui.clone();
            match save_config(&config, config_path.as_deref()) {
                Ok(()) => ActionResult::TuiConfigSaved,
                Err(e) => ActionResult::Error {
                    context: "save config".to_string(),
                    message: e.user_message(),
                },
            }
        }
    }
}

fn build_candidates(
    discovery: &DiscoveryPayload,
) -> Vec<pulsos_core::sync::correlate::CorrelationCandidate> {
    let discovery_results = DiscoveryResults {
        github: discovery.github.clone(),
        railway: discovery.railway.clone(),
        vercel: discovery.vercel.clone(),
    };
    build_correlations(&discovery_results)
}

fn load_or_default(config_path: Option<&Path>) -> PulsosConfig {
    load_config(config_path).unwrap_or_default()
}

/// Check whether `pid` is a running instance of this binary.
///
/// On Linux, `/proc/<pid>/exe` is a symlink to the executable — compare it
/// against `current_exe()` for a reliable identity check.
///
/// On macOS and other Unix systems without `/proc`, the PID file is written
/// exclusively by our daemon (mode 0600) so a live PID is almost certainly
/// ours; we verify only that the process is still alive via `kill -0`.
#[cfg(unix)]
fn verify_pid_is_daemon(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        let our_exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => return true, // cannot verify; proceed optimistically
        };
        match std::fs::read_link(format!("/proc/{pid}/exe")) {
            Ok(proc_exe) => proc_exe == our_exe,
            Err(_) => false, // process not found or permission denied
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Trust the 0600 PID file; verify the process is still alive.
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
