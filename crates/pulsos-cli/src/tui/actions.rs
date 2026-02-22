use std::path::{Path, PathBuf};
use std::sync::Arc;

use pulsos_core::auth::credential_store::{CredentialStore, FallbackStore};
use pulsos_core::auth::resolve::TokenResolver;
use pulsos_core::auth::validate::validate_token;
use pulsos_core::auth::PlatformKind;
use pulsos_core::cache::store::CacheStore;
use pulsos_core::config::types::PulsosConfig;
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
        config: PulsosConfig,
    },
    Error {
        context: String,
        message: String,
    },
}

pub async fn run_worker(
    config_path: Option<PathBuf>,
    mut rx: tokio::sync::mpsc::Receiver<ActionRequest>,
    tx: tokio::sync::mpsc::Sender<ActionResult>,
) {
    let cache = match CacheStore::open_or_temporary() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            let _ = tx
                .send(ActionResult::Error {
                    context: "init".to_string(),
                    message: e.user_message(),
                })
                .await;
            return;
        }
    };
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
                    PlatformKind::GitHub => {
                        let client = GitHubClient::new(token, cache.clone());
                        match client.discover().await {
                            Ok(resources) => {
                                payload.github = resources
                                    .into_iter()
                                    .filter(|r| !r.archived && !r.disabled)
                                    .collect();
                            }
                            Err(err) => payload
                                .warnings
                                .push(format!("GitHub discovery failed: {}", err.user_message())),
                        }
                    }
                    PlatformKind::Railway => {
                        let client = RailwayClient::new(token, cache.clone());
                        match client.discover().await {
                            Ok(resources) => {
                                payload.railway = resources
                                    .into_iter()
                                    .filter(|r| !r.archived && !r.disabled)
                                    .collect();
                            }
                            Err(err) => payload
                                .warnings
                                .push(format!("Railway discovery failed: {}", err.user_message())),
                        }
                    }
                    PlatformKind::Vercel => {
                        let client = VercelClient::new(token, cache.clone());
                        match client.discover_with_links().await {
                            Ok(resources) => {
                                payload.vercel = resources
                                    .into_iter()
                                    .filter(|(r, _)| !r.archived && !r.disabled)
                                    .collect();
                            }
                            Err(err) => payload
                                .warnings
                                .push(format!("Vercel discovery failed: {}", err.user_message())),
                        }
                    }
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
                config: merged,
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
