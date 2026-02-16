Pulsos Technical Design Document

Version: 1.0.0 Date: February 15, 2026 Status: Draft Companion to: Pulsos PRD v3.0

1. Project Structure

pulsos/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── .github/
│   └── workflows/
│       ├── ci.yml                # Test + lint + audit on every PR
│       ├── release.yml           # Cross-compile + publish binaries on tag
│       └── audit.yml             # Weekly cargo audit + cargo deny
├── crates/
│   ├── pulsos-core/              # Library crate: all business logic
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config/           # TOML config parsing + validation
│   │       │   ├── mod.rs
│   │       │   ├── types.rs      # Config structs (serde)
│   │       │   └── validate.rs   # Validation logic
│   │       ├── platform/         # Platform adapters
│   │       │   ├── mod.rs        # PlatformAdapter trait
│   │       │   ├── github/
│   │       │   │   ├── mod.rs
│   │       │   │   ├── client.rs # reqwest calls to api.github.com
│   │       │   │   └── types.rs  # GitHub API response structs
│   │       │   ├── railway/
│   │       │   │   ├── mod.rs
│   │       │   │   ├── client.rs # GraphQL calls to backboard.railway.com
│   │       │   │   ├── types.rs  # Railway API response structs
│   │       │   │   └── queries/  # .graphql files for graphql_client codegen
│   │       │   │       ├── projects.graphql
│   │       │   │       ├── deployments.graphql
│   │       │   │       └── service_instance.graphql
│   │       │   └── vercel/
│   │       │       ├── mod.rs
│   │       │       ├── client.rs # reqwest calls to api.vercel.com
│   │       │       └── types.rs  # Vercel API response structs
│   │       ├── domain/           # Unified domain types
│   │       │   ├── mod.rs
│   │       │   ├── project.rs    # UnifiedProject, CorrelationMapping
│   │       │   ├── deployment.rs # UnifiedDeployment, DeploymentStatus
│   │       │   ├── health.rs     # HealthScore computation
│   │       │   └── event.rs      # DeploymentEvent timeline
│   │       ├── correlation/      # Correlation engine
│   │       │   ├── mod.rs
│   │       │   ├── sha_match.rs  # Exact SHA matching (GitHub ↔ Vercel)
│   │       │   ├── heuristic.rs  # Timestamp/branch heuristic (GitHub ↔ Railway)
│   │       │   └── confidence.rs # Confidence scoring
│   │       ├── cache/            # Cache layer
│   │       │   ├── mod.rs
│   │       │   ├── store.rs      # sled wrapper with TTL
│   │       │   └── keys.rs       # Cache key design
│   │       ├── auth/             # Authentication
│   │       │   ├── mod.rs
│   │       │   ├── keyring.rs    # OS keyring via keyring crate
│   │       │   ├── detect.rs     # Token detection from CLI configs
│   │       │   ├── validate.rs   # Scope validation per platform
│   │       │   └── encrypted.rs  # Fallback encrypted file store
│   │       ├── scheduler/        # Poll scheduling + rate limit budget
│   │       │   ├── mod.rs
│   │       │   ├── budget.rs     # Rate limit budget manager
│   │       │   └── poller.rs     # Staggered polling loop
│   │       └── error.rs          # Error types (thiserror)
│   ├── pulsos-cli/               # Binary crate: CLI + TUI
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/         # One file per top-level command
│   │       │   ├── mod.rs
│   │       │   ├── status.rs     # pulsos status
│   │       │   ├── auth.rs       # pulsos auth
│   │       │   ├── repos.rs      # pulsos repos
│   │       │   ├── views.rs      # pulsos views
│   │       │   ├── doctor.rs     # pulsos doctor
│   │       │   └── config.rs     # pulsos config
│   │       ├── output/           # Output formatters
│   │       │   ├── mod.rs
│   │       │   ├── table.rs      # tabled table formatter
│   │       │   ├── json.rs       # JSON output
│   │       │   └── compact.rs    # Compact single-line output
│   │       ├── tui/              # ratatui TUI (Phase 6)
│   │       │   ├── mod.rs
│   │       │   ├── app.rs        # App state machine
│   │       │   ├── tabs/
│   │       │   │   ├── unified.rs
│   │       │   │   ├── platform.rs
│   │       │   │   └── health.rs
│   │       │   ├── widgets/      # Custom widgets
│   │       │   └── event.rs      # Input event handling
│   │       └── wizard/           # First-run wizard
│   │           ├── mod.rs
│   │           ├── auth_step.rs
│   │           ├── discovery.rs
│   │           └── correlation.rs
│   └── pulsos-test/              # Test utilities + fixtures
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── mock_server.rs    # wiremock-based mock API servers
│           ├── fixtures/         # JSON response fixtures
│           │   ├── github/
│           │   │   ├── workflow_runs.json
│           │   │   ├── workflow_jobs.json
│           │   │   ├── repos.json
│           │   │   └── orgs.json
│           │   ├── railway/
│           │   │   ├── projects.json
│           │   │   ├── deployments.json
│           │   │   └── service_instance.json
│           │   └── vercel/
│           │       ├── deployments.json
│           │       ├── projects.json
│           │       └── teams.json
│           └── builders.rs       # Test data builders
├── schema/
│   └── railway.graphql           # Railway GraphQL schema (for codegen)
├── docs/
│   ├── ARCHITECTURE.md
│   └── CONTRIBUTING.md
├── release/
│   └── homebrew/
│       └── pulsos.rb             # Homebrew formula template
└── README.md


1.1 Workspace Cargo.toml

[workspace]
resolver = "2"
members = [
    "crates/pulsos-core",
    "crates/pulsos-cli",
    "crates/pulsos-test",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "MIT"
repository = "https://github.com/lambdahq/pulsos"

[workspace.dependencies]
# HTTP + async
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# CLI
clap = { version = "4", features = ["derive", "env"] }

# Cache
sled = "0.34"

# Time
chrono = { version = "0.4", features = ["serde"] }

# Auth
keyring = "3"
secrecy = { version = "0.8", features = ["serde"] }

# Interactive
dialoguer = "0.11"
fuzzy-matcher = "0.3"

# Table output
tabled = "0.16"

# TUI (Phase 6)
ratatui = "0.29"
crossterm = "0.28"

# GraphQL
graphql_client = "0.14"

# Error handling
thiserror = "2"
anyhow = "1"

# Paths
dirs = "5"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Testing
wiremock = "0.6"


1.2 Core Crate Dependencies

# crates/pulsos-core/Cargo.toml
[package]
name = "pulsos-core"
version.workspace = true
edition.workspace = true

[dependencies]
reqwest.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
sled.workspace = true
chrono.workspace = true
keyring.workspace = true
secrecy.workspace = true
graphql_client.workspace = true
dirs.workspace = true
thiserror.workspace = true
anyhow.workspace = true
tracing.workspace = true

[dev-dependencies]
wiremock.workspace = true
tokio = { workspace = true, features = ["test-util"] }


2. API Response Types (Wire Format)

These are the exact serde::Deserialize structs matching the JSON that each platform returns. They live in platform/{name}/types.rs and are never exposed outside the platform adapter module. Each adapter converts these into unified domain types.

2.1 GitHub API Response Types

// crates/pulsos-core/src/platform/github/types.rs

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// GET /repos/{owner}/{repo}/actions/runs
#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowRunsResponse {
    pub total_count: u64,
    pub workflow_runs: Vec<WorkflowRun>,
}

/// A single workflow run from the GitHub Actions API.
/// Reference: https://docs.github.com/en/rest/actions/workflow-runs
#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowRun {
    pub id: u64,
    pub name: Option<String>,
    pub head_branch: Option<String>,
    pub head_sha: String,
    pub path: Option<String>,           // ".github/workflows/ci.yml@main"
    pub run_number: u64,
    pub event: String,                   // "push", "pull_request", "workflow_dispatch", etc.
    pub display_title: Option<String>,   // Commit message or PR title
    pub status: GhRunStatus,
    pub conclusion: Option<GhConclusion>,
    pub workflow_id: u64,
    pub html_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub run_started_at: Option<DateTime<Utc>>,
    pub actor: Option<GhActor>,
}

/// GitHub workflow run status values.
/// Status describes the *lifecycle* state (is it running?).
/// Conclusion describes the *outcome* (did it pass?).
///
/// Status transitions: queued → in_progress → completed
/// Conclusion is null until status == completed.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GhRunStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
}

/// GitHub workflow run conclusion values.
/// Only set when status == Completed.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GhConclusion {
    Success,
    Failure,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
    Neutral,
    Stale,
    StartupFailure,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhActor {
    pub login: String,
    pub id: u64,
    pub avatar_url: Option<String>,
}

/// GET /repos/{owner}/{repo}/actions/runs/{id}/jobs
#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowJobsResponse {
    pub total_count: u64,
    pub jobs: Vec<WorkflowJob>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowJob {
    pub id: u64,
    pub run_id: u64,
    pub head_sha: String,
    pub status: GhRunStatus,
    pub conclusion: Option<GhConclusion>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub name: String,
    pub steps: Option<Vec<WorkflowStep>>,
    pub html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowStep {
    pub name: String,
    pub status: GhRunStatus,
    pub conclusion: Option<GhConclusion>,
    pub number: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// GET /user/orgs
#[derive(Debug, Deserialize)]
pub(crate) struct GhOrg {
    pub login: String,
    pub id: u64,
    pub description: Option<String>,
}

/// GET /user/repos or GET /orgs/{org}/repos
#[derive(Debug, Deserialize)]
pub(crate) struct GhRepo {
    pub id: u64,
    pub full_name: String,         // "myorg/my-saas"
    pub name: String,              // "my-saas"
    pub private: bool,
    pub archived: bool,
    pub disabled: bool,
    pub default_branch: Option<String>,
    pub html_url: String,
    pub permissions: Option<GhRepoPermissions>,
    pub owner: GhOwner,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhRepoPermissions {
    pub admin: bool,
    pub push: bool,
    pub pull: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhOwner {
    pub login: String,
    pub id: u64,
    #[serde(rename = "type")]
    pub owner_type: String,   // "User" or "Organization"
}

/// Rate limit tracking from response headers.
/// Not a JSON response — parsed from HTTP headers.
#[derive(Debug, Clone)]
pub(crate) struct GhRateLimit {
    pub limit: u32,
    pub remaining: u32,
    pub reset: DateTime<Utc>,
    pub used: u32,
}


2.2 Railway GraphQL Response Types

// crates/pulsos-core/src/platform/railway/types.rs

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Railway uses Relay-style pagination: edges → node.
/// All list queries return this shape.
#[derive(Debug, Deserialize)]
pub(crate) struct Connection<T> {
    pub edges: Vec<Edge<T>>,
    #[serde(default)]
    pub page_info: Option<PageInfo>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Edge<T> {
    pub node: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

/// Root response wrapper for GraphQL.
#[derive(Debug, Deserialize)]
pub(crate) struct GqlResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GqlError>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GqlError {
    pub message: String,
    pub extensions: Option<serde_json::Value>,
}

// ── Query response shapes ──

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectsData {
    pub projects: Connection<RwProject>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RwProject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub services: Connection<RwService>,
    pub environments: Connection<RwEnvironment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RwService {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RwEnvironment {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServiceInstanceData {
    pub service_instance: RwServiceInstance,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RwServiceInstance {
    pub id: String,
    pub service_name: String,
    pub start_command: Option<String>,
    pub build_command: Option<String>,
    pub root_directory: Option<String>,
    pub healthcheck_path: Option<String>,
    pub region: Option<String>,
    pub num_replicas: Option<u32>,
    pub restart_policy_type: Option<String>,
    pub restart_policy_max_retries: Option<u32>,
    pub latest_deployment: Option<RwDeployment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeploymentsData {
    pub deployments: Connection<RwDeployment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RwDeployment {
    pub id: String,
    pub status: RwDeploymentStatus,
    pub created_at: DateTime<Utc>,
    pub static_url: Option<String>,
}

/// Railway deployment status values.
///
/// CRITICAL NOTE: These represent the *deployment* outcome,
/// NOT the service's current runtime health. Railway does not
/// continuously monitor service health after deployment completes.
///
/// Status lifecycle:
///   QUEUED → INITIALIZING → BUILDING → DEPLOYING → SUCCESS
///                                                 → FAILED
///                                                 → CRASHED
///
/// Terminal states: SUCCESS, FAILED, CRASHED, REMOVED, SKIPPED
/// Blocking states: NEEDS_APPROVAL, WAITING
/// Dormant states: SLEEPING
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum RwDeploymentStatus {
    Building,
    Crashed,
    Deploying,
    Failed,
    Initializing,
    NeedsApproval,
    Queued,
    Removed,
    Removing,
    Skipped,
    Sleeping,
    Success,
    Waiting,
}

/// Workspaces are obtained from the me query or teams query.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RwTeam {
    pub id: String,
    pub name: String,
}


2.3 Vercel API Response Types

// crates/pulsos-core/src/platform/vercel/types.rs

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// GET /v6/deployments (list response)
/// The response wraps deployments in a { deployments: [...], pagination: {...} } shape.
#[derive(Debug, Deserialize)]
pub(crate) struct DeploymentsResponse {
    pub deployments: Vec<VcDeployment>,
    pub pagination: Option<VcPagination>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VcPagination {
    pub count: u64,
    pub next: Option<u64>,    // Timestamp for cursor-based pagination
    pub prev: Option<u64>,
}

/// A single Vercel deployment.
/// Reference: https://vercel.com/docs/rest-api/endpoints/deployments
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VcDeployment {
    pub uid: String,              // "dpl_xxx" — unique deployment ID
    pub name: String,             // Project name
    pub url: Option<String>,      // "my-app-abc123.vercel.app"
    pub created: u64,             // Unix timestamp in milliseconds
    pub state: Option<VcState>,
    pub ready_state: Option<VcState>,
    #[serde(rename = "type")]
    pub deploy_type: Option<String>,  // "LAMBDAS"
    pub creator: Option<VcCreator>,
    pub meta: Option<VcMeta>,
    pub target: Option<String>,       // "production" or null (preview)
    pub alias_assigned: Option<serde_json::Value>, // Timestamp or bool
    pub building_at: Option<u64>,
    pub ready: Option<u64>,
}

/// Vercel deployment meta — this is where git information lives.
/// Auto-populated by the GitHub integration for git-connected projects.
///
/// CRITICAL: This is the primary correlation mechanism with GitHub.
/// meta.githubCommitSha gives us exact SHA matching.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VcMeta {
    pub github_commit_sha: Option<String>,
    pub github_commit_ref: Option<String>,      // Branch name
    pub github_commit_message: Option<String>,
    pub github_commit_author_name: Option<String>,
    pub github_commit_org: Option<String>,
    pub github_commit_repo: Option<String>,
    pub github_deployment: Option<String>,       // "1" if via GitHub integration
}

/// Vercel deployment state.
///
/// State transitions: QUEUED → BUILDING → READY
///                                      → ERROR
///                                      → CANCELED
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum VcState {
    Queued,
    Building,
    Ready,
    Error,
    Canceled,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VcCreator {
    pub uid: String,
    pub username: Option<String>,
    pub email: Option<String>,
}

/// GET /v9/projects (list response)
#[derive(Debug, Deserialize)]
pub(crate) struct ProjectsResponse {
    pub projects: Vec<VcProject>,
    pub pagination: Option<VcPagination>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VcProject {
    pub id: String,
    pub name: String,
    pub framework: Option<String>,   // "nextjs", "vite", etc.
    pub link: Option<VcProjectLink>,
    pub latest_deployments: Option<Vec<VcDeployment>>,
    pub account_id: Option<String>,
}

/// Project link — tells us which GitHub repo is connected.
/// This is the key field for automatic GitHub ↔ Vercel correlation.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VcProjectLink {
    #[serde(rename = "type")]
    pub link_type: Option<String>,  // "github", "gitlab", "bitbucket"
    pub repo: Option<String>,       // "myorg/my-saas"
    pub repo_id: Option<u64>,
    pub org: Option<String>,        // "myorg"
}

/// GET /v2/teams
#[derive(Debug, Deserialize)]
pub(crate) struct TeamsResponse {
    pub teams: Vec<VcTeam>,
    pub pagination: Option<VcPagination>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VcTeam {
    pub id: String,
    pub name: String,
    pub slug: String,
}


3. Unified Domain Types

These are the internal types that the rest of the application works with. Platform adapters convert wire types → domain types. These are the types that the CLI, TUI, cache, and correlation engine all share.

// crates/pulsos-core/src/domain/deployment.rs

use chrono::{DateTime, Utc};

/// The universal deployment status across all platforms.
/// Each platform's native status maps into one of these.
///
/// This is the most important enum in the codebase.
/// Every display, filter, and health calculation uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeploymentStatus {
    /// Waiting to start (GH: queued/waiting, RW: QUEUED/WAITING, VC: QUEUED)
    Queued,
    /// Actively building/running (GH: in_progress, RW: BUILDING/DEPLOYING/INITIALIZING, VC: BUILDING)
    InProgress,
    /// Completed successfully (GH: completed+success, RW: SUCCESS, VC: READY)
    Success,
    /// Completed with failure (GH: completed+failure, RW: FAILED/CRASHED, VC: ERROR)
    Failed,
    /// Cancelled by user (GH: completed+cancelled, RW: REMOVED, VC: CANCELED)
    Cancelled,
    /// Skipped (GH: completed+skipped, RW: SKIPPED)
    Skipped,
    /// Needs manual action (GH: completed+action_required, RW: NEEDS_APPROVAL)
    ActionRequired,
    /// Dormant/sleeping (RW: SLEEPING only)
    Sleeping,
    /// Unknown — platform returned a value we don't recognize
    Unknown(String),
}

/// Mapping table for platform-specific → unified status.
///
/// GitHub: status + conclusion → DeploymentStatus
///   queued / waiting / requested / pending  → Queued
///   in_progress                             → InProgress
///   completed + success                     → Success
///   completed + failure / startup_failure   → Failed
///   completed + cancelled                   → Cancelled
///   completed + skipped                     → Skipped
///   completed + action_required             → ActionRequired
///   completed + timed_out                   → Failed
///   completed + neutral / stale             → Success (soft pass)
///
/// Railway: status → DeploymentStatus
///   QUEUED / WAITING                        → Queued
///   BUILDING / DEPLOYING / INITIALIZING     → InProgress
///   SUCCESS                                 → Success
///   FAILED / CRASHED                        → Failed
///   REMOVED / REMOVING                      → Cancelled
///   SKIPPED                                 → Skipped
///   NEEDS_APPROVAL                          → ActionRequired
///   SLEEPING                                → Sleeping
///
/// Vercel: state → DeploymentStatus
///   QUEUED                                  → Queued
///   BUILDING                                → InProgress
///   READY                                   → Success
///   ERROR                                   → Failed
///   CANCELED                                → Cancelled

/// Source platform for a deployment event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    GitHub,
    Railway,
    Vercel,
}

/// A single deployment event, normalized across platforms.
#[derive(Debug, Clone)]
pub struct DeploymentEvent {
    /// Platform-specific unique ID
    pub id: String,
    /// Which platform this came from
    pub platform: Platform,
    /// Normalized status
    pub status: DeploymentStatus,
    /// Git commit SHA (exact for GitHub/Vercel, absent for Railway)
    pub commit_sha: Option<String>,
    /// Git branch
    pub branch: Option<String>,
    /// Commit message or workflow display title
    pub title: Option<String>,
    /// Who triggered this deployment
    pub actor: Option<String>,
    /// When this event was created on the platform
    pub created_at: DateTime<Utc>,
    /// When this event was last updated
    pub updated_at: Option<DateTime<Utc>>,
    /// Duration in seconds (completed_at - started_at)
    pub duration_secs: Option<u64>,
    /// Platform-specific URL for viewing in browser
    pub url: Option<String>,
    /// Platform-specific extra data (workflow name, service name, etc.)
    pub metadata: EventMetadata,
}

/// Platform-specific metadata that doesn't fit the unified model.
#[derive(Debug, Clone, Default)]
pub struct EventMetadata {
    /// GitHub: workflow name ("CI", "Deploy")
    pub workflow_name: Option<String>,
    /// GitHub: trigger event ("push", "pull_request")
    pub trigger_event: Option<String>,
    /// Railway: service name
    pub service_name: Option<String>,
    /// Railway: environment name
    pub environment_name: Option<String>,
    /// Vercel: deployment URL ("my-app-abc123.vercel.app")
    pub preview_url: Option<String>,
    /// Vercel: target ("production" or preview)
    pub deploy_target: Option<String>,
}


// crates/pulsos-core/src/domain/project.rs

use super::deployment::{DeploymentEvent, Platform};
use chrono::{DateTime, Utc};

/// A unified project — the core abstraction that maps across platforms.
/// Created during first-run wizard and stored in config.toml.
#[derive(Debug, Clone)]
pub struct UnifiedProject {
    /// User-facing name (e.g., "my-saas")
    pub name: String,
    /// GitHub binding (if connected)
    pub github: Option<GitHubBinding>,
    /// Railway binding (if connected)
    pub railway: Option<RailwayBinding>,
    /// Vercel binding (if connected)
    pub vercel: Option<VercelBinding>,
    /// Most recent events per platform (populated at runtime)
    pub events: Vec<DeploymentEvent>,
    /// Computed health score (0–100, populated at runtime)
    pub health_score: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct GitHubBinding {
    pub repo_full_name: String,      // "myorg/my-saas"
    pub workflows: Vec<String>,      // ["ci.yml", "deploy.yml"] or empty for all
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RailwayBinding {
    pub project_id: String,
    pub project_name: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub services: Vec<RailwayServiceRef>,
    pub environment_id: String,
    pub environment_name: String,     // "production", "staging"
}

#[derive(Debug, Clone)]
pub struct RailwayServiceRef {
    pub service_id: String,
    pub service_name: String,
}

#[derive(Debug, Clone)]
pub struct VercelBinding {
    pub project_id: String,
    pub project_name: String,
    pub team_id: Option<String>,
    pub team_name: Option<String>,
    pub linked_repo: Option<String>,  // "myorg/my-saas" (from project.link.repo)
    pub include_previews: bool,
}

/// Correlation confidence level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// SHA match — GitHub ↔ Vercel only
    Exact,
    /// Explicit config mapping + timestamp match
    High,
    /// Timestamp-only heuristic
    Low,
    /// No correlation found
    Unmatched,
}

/// A correlated deployment event — links events across platforms
/// for the same commit/deployment.
#[derive(Debug, Clone)]
pub struct CorrelatedEvent {
    pub commit_sha: Option<String>,
    pub github: Option<DeploymentEvent>,
    pub railway: Option<DeploymentEvent>,
    pub vercel: Option<DeploymentEvent>,
    pub confidence: Confidence,
    pub timestamp: DateTime<Utc>,
}


// crates/pulsos-core/src/domain/health.rs

use super::deployment::DeploymentStatus;

/// Health score computation for a unified project.
///
/// Score: 0–100
/// Weights:
///   GitHub CI success rate (last 10 runs): 40%
///   Railway latest deployment status:      35%
///   Vercel latest deployment status:       25%
///
/// If a platform is not connected, its weight is redistributed
/// proportionally to the connected platforms.
pub struct HealthCalculator;

impl HealthCalculator {
    pub fn compute(
        github_runs: &[DeploymentStatus],
        railway_status: Option<DeploymentStatus>,
        vercel_status: Option<DeploymentStatus>,
    ) -> u8 {
        let mut total_weight = 0.0_f64;
        let mut weighted_score = 0.0_f64;

        // GitHub: success rate of last N runs
        if !github_runs.is_empty() {
            let success_count = github_runs.iter()
                .filter(|s| matches!(s, DeploymentStatus::Success))
                .count();
            let rate = success_count as f64 / github_runs.len() as f64;
            weighted_score += rate * 40.0;
            total_weight += 40.0;
        }

        // Railway: binary — latest deployment status
        if let Some(status) = railway_status {
            let score = Self::status_score(status);
            weighted_score += score * 35.0;
            total_weight += 35.0;
        }

        // Vercel: binary — latest deployment status
        if let Some(status) = vercel_status {
            let score = Self::status_score(status);
            weighted_score += score * 25.0;
            total_weight += 25.0;
        }

        if total_weight == 0.0 {
            return 0;
        }

        // Normalize to 0–100
        ((weighted_score / total_weight) * 100.0).round() as u8
    }

    fn status_score(status: DeploymentStatus) -> f64 {
        match status {
            DeploymentStatus::Success => 1.0,
            DeploymentStatus::InProgress | DeploymentStatus::Queued => 0.7,
            DeploymentStatus::Sleeping => 0.5,
            DeploymentStatus::Skipped | DeploymentStatus::Cancelled => 0.5,
            DeploymentStatus::ActionRequired => 0.3,
            DeploymentStatus::Failed => 0.0,
            DeploymentStatus::Unknown(_) => 0.5,
        }
    }
}


4. Platform Adapter Trait

// crates/pulsos-core/src/platform/mod.rs

use crate::domain::deployment::DeploymentEvent;
use crate::error::PulsosError;
use async_trait::async_trait;

/// Every platform adapter implements this trait.
/// The trait uses domain types exclusively — no wire types leak out.
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Fetch the latest deployment events for all tracked resources.
    /// Returns events sorted by created_at descending.
    async fn fetch_events(
        &self,
        tracked: &[TrackedResource],
    ) -> Result<Vec<DeploymentEvent>, PulsosError>;

    /// Discover all available resources (repos, projects, etc.)
    /// Used during first-run wizard and `repos sync`.
    async fn discover(&self) -> Result<Vec<DiscoveredResource>, PulsosError>;

    /// Validate that the stored token is still valid and has correct scopes.
    async fn validate_auth(&self) -> Result<AuthStatus, PulsosError>;

    /// Return the current rate limit status.
    async fn rate_limit_status(&self) -> Result<RateLimitInfo, PulsosError>;
}

/// A resource the user has chosen to track.
#[derive(Debug, Clone)]
pub struct TrackedResource {
    pub platform_id: String,     // repo full_name, project ID, etc.
    pub display_name: String,
    pub group: Option<String>,   // org, workspace, team name
}

/// A resource discovered during scanning.
#[derive(Debug, Clone)]
pub struct DiscoveredResource {
    pub platform_id: String,
    pub display_name: String,
    pub group: String,
    pub group_type: String,     // "organization", "workspace", "team"
    pub archived: bool,
    pub disabled: bool,
}

/// Auth validation result.
#[derive(Debug)]
pub struct AuthStatus {
    pub valid: bool,
    pub identity: String,           // "@vivallo", "v@lambda.co", "lambda-team"
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub warnings: Vec<String>,      // "Token has unnecessary write scope"
}

/// Rate limit info for display in `doctor`.
#[derive(Debug)]
pub struct RateLimitInfo {
    pub limit: u32,
    pub remaining: u32,
    pub resets_at: chrono::DateTime<chrono::Utc>,
    pub percentage_used: f32,
}


5. Rate Limit Budget

This is the most critical operational constraint. Without a budget, Pulsos will exhaust GitHub's 5,000 requests/hour within minutes.

5.1 API Call Cost Per Poll Cycle

One "poll cycle" fetches fresh data for all tracked resources. Here is the cost per platform:

GitHub

Call

Per

Example (20 repos)

Notes

GET /repos/{o}/{r}/actions/runs?per_page=5

repo

20 calls

Only fetch last 5 runs

GET /repos/{o}/{r}/actions/runs/{id}/jobs

run (detail)

0 calls*

Only on user drill-down, not in poll

Total per cycle

20 calls

*Job-level detail is fetched on-demand when a user selects a specific run in the TUI, not during background polling.

Railway

Call

Per

Example (8 services)

Notes

Single GraphQL query with nested fields

all services

1 call

GraphQL returns all projects + services + latest deployment in one query

Total per cycle

1 call

Railway's GraphQL API is extremely efficient for Pulsos — one query can return all projects, all services, and each service's latest deployment in a single HTTP POST.

Vercel

Call

Per

Example (4 projects)

Notes

GET /v6/deployments?projectId={id}&limit=5

project

4 calls

Last 5 deployments per project

Total per cycle

4 calls

Total Cost Per Cycle: 25 API calls

5.2 Budget Allocation

GitHub budget: 5,000 requests/hour

Poll Mode

Interval

Calls/Cycle

Cycles/Hour

Total Calls/Hour

Budget Usage

pulsos status (one-shot)

—

20

1

20

0.4%

--watch (default 5s)

30s*

20

120

2,400

48%

--watch (aggressive)

10s

20

360

7,200

144% — OVER

*The watch interval for GitHub is 30 seconds minimum, not the TUI's visual refresh rate. The TUI refreshes cached data every 5 seconds, but only actually polls GitHub every 30 seconds.

Critical design decision: Decouple TUI refresh rate from API poll rate.

TUI refresh (renders cached data):     every 5 seconds
GitHub API poll (fetches new data):    every 30 seconds minimum
Railway API poll:                      every 15 seconds
Vercel API poll:                       every 15 seconds


The TUI always has something to show (cached data), so the user never sees a blank screen. Fresh data replaces cached data asynchronously when it arrives.

5.3 Conditional Requests (ETags)

GitHub supports conditional requests via If-None-Match / ETag headers. If the data hasn't changed since the last request, GitHub returns 304 Not Modified and does not count it against the rate limit.

// crates/pulsos-core/src/platform/github/client.rs

/// Each endpoint+params combination gets its own ETag.
/// On 304 Not Modified: serve cached data, no rate limit cost.
/// On 200 OK: store new ETag + data in cache.
struct CachedRequest {
    url: String,
    etag: Option<String>,       // From last response's ETag header
    last_data: serde_json::Value,
    last_fetched: DateTime<Utc>,
}

async fn fetch_with_etag(
    &self,
    url: &str,
    cached: &mut CachedRequest,
) -> Result<Option<serde_json::Value>, PulsosError> {
    let mut req = self.client.get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(etag) = &cached.etag {
        req = req.header("If-None-Match", etag);
    }

    let resp = req.send().await?;

    // Update rate limit tracking from headers
    self.update_rate_limit(&resp);

    match resp.status().as_u16() {
        304 => {
            // Data unchanged — free request!
            cached.last_fetched = Utc::now();
            Ok(None)  // Caller uses cached data
        }
        200 => {
            if let Some(etag) = resp.headers().get("etag") {
                cached.etag = Some(etag.to_str()?.to_owned());
            }
            let data: serde_json::Value = resp.json().await?;
            cached.last_data = data.clone();
            cached.last_fetched = Utc::now();
            Ok(Some(data))
        }
        _ => Err(PulsosError::api_error(resp)),
    }
}


With ETags, the effective rate limit cost drops dramatically for stable repos:

Scenario

ETags hit rate

Effective calls/hour (20 repos, 30s poll)

All repos active (constant pushes)

0%

2,400

Mixed activity (typical)

70%

720

Quiet period (no pushes)

95%

120

5.4 Adaptive Polling

// crates/pulsos-core/src/scheduler/budget.rs

pub struct RateLimitBudget {
    github_remaining: u32,
    github_limit: u32,
    github_reset: DateTime<Utc>,
}

impl RateLimitBudget {
    /// Returns the recommended poll interval in seconds.
    /// Backs off as rate limit is consumed.
    pub fn recommended_interval(&self) -> u64 {
        let pct_remaining = self.github_remaining as f64 / self.github_limit as f64;

        match pct_remaining {
            p if p > 0.5 => 30,    // Plenty of budget: normal polling
            p if p > 0.2 => 60,    // Getting low: slow down
            p if p > 0.1 => 120,   // Critical: very slow
            _ => {
                // Exhausted: calculate seconds until reset
                let until_reset = (self.github_reset - Utc::now())
                    .num_seconds()
                    .max(60);
                until_reset as u64
            }
        }
    }
}


5.5 Staggered Polling

Instead of polling all 20 repos simultaneously every 30 seconds, Pulsos staggers requests:

t=0s:   Poll repos 1-5   (5 requests)
t=7s:   Poll repos 6-10  (5 requests)
t=14s:  Poll repos 11-15 (5 requests)
t=21s:  Poll repos 16-20 (5 requests)
t=30s:  Poll repos 1-5   (next cycle)


This spreads the load evenly and avoids burst patterns that trigger rate limiting on any platform.

6. Cache Key Design

// crates/pulsos-core/src/cache/keys.rs

/// Cache keys use a hierarchical namespace:
///   {platform}:{resource_type}:{resource_id}:{sub_resource}
///
/// Examples:
///   github:runs:myorg/my-saas          → Last 5 workflow runs
///   github:jobs:myorg/my-saas:12345    → Jobs for run 12345
///   railway:deployments:project-uuid   → Last 5 deployments
///   railway:instance:service-uuid:env-uuid → Service instance
///   vercel:deployments:project-id      → Last 5 deployments
///   vercel:projects:team-id            → Project list for team
///
///   meta:github:rate_limit             → Current rate limit state
///   meta:github:etag:myorg/my-saas     → ETag for last request
///   config:projects                    → Serialized project list

pub fn github_runs_key(repo: &str) -> String {
    format!("github:runs:{repo}")
}

pub fn github_etag_key(repo: &str) -> String {
    format!("meta:github:etag:{repo}")
}

pub fn railway_deployments_key(project_id: &str) -> String {
    format!("railway:deployments:{project_id}")
}

pub fn railway_instance_key(service_id: &str, env_id: &str) -> String {
    format!("railway:instance:{service_id}:{env_id}")
}

pub fn vercel_deployments_key(project_id: &str) -> String {
    format!("vercel:deployments:{project_id}")
}


6.1 Cache Entry Format

// crates/pulsos-core/src/cache/store.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub data: T,
    pub fetched_at: DateTime<Utc>,
    pub ttl_secs: u64,
    pub etag: Option<String>,
}

impl<T> CacheEntry<T> {
    pub fn is_fresh(&self) -> bool {
        let age = Utc::now() - self.fetched_at;
        age.num_seconds() < self.ttl_secs as i64
    }

    pub fn is_stale(&self) -> bool {
        !self.is_fresh() && !self.is_expired()
    }

    pub fn is_expired(&self) -> bool {
        let max_staleness = self.ttl_secs * 120; // 2x TTL = max staleness
        let age = Utc::now() - self.fetched_at;
        age.num_seconds() > max_staleness as i64
    }

    pub fn age(&self) -> Duration {
        let secs = (Utc::now() - self.fetched_at).num_seconds().max(0) as u64;
        Duration::from_secs(secs)
    }
}


7. Correlation Engine Algorithm

// crates/pulsos-core/src/correlation/mod.rs

use crate::domain::deployment::DeploymentEvent;
use crate::domain::project::{Confidence, CorrelatedEvent, UnifiedProject};
use chrono::Duration;

const TIMESTAMP_WINDOW_SECS: i64 = 120;

pub fn correlate_events(project: &UnifiedProject) -> Vec<CorrelatedEvent> {
    let mut github_events: Vec<&DeploymentEvent> = project.events.iter()
        .filter(|e| e.platform == Platform::GitHub)
        .collect();
    let railway_events: Vec<&DeploymentEvent> = project.events.iter()
        .filter(|e| e.platform == Platform::Railway)
        .collect();
    let vercel_events: Vec<&DeploymentEvent> = project.events.iter()
        .filter(|e| e.platform == Platform::Vercel)
        .collect();

    let mut correlated = Vec::new();

    for gh_event in &github_events {
        let sha = &gh_event.commit_sha;
        let mut entry = CorrelatedEvent {
            commit_sha: sha.clone(),
            github: Some((*gh_event).clone()),
            railway: None,
            vercel: None,
            confidence: Confidence::Unmatched,
            timestamp: gh_event.created_at,
        };

        // Step 1: Exact SHA match with Vercel
        if let Some(sha) = sha {
            if let Some(vc) = vercel_events.iter()
                .find(|v| v.commit_sha.as_deref() == Some(sha.as_str()))
            {
                entry.vercel = Some((*vc).clone());
                entry.confidence = Confidence::Exact;
            }
        }

        // Step 2: Heuristic match with Railway
        // Railway has no SHA — use timestamp proximity + branch
        if let Some(rw) = railway_events.iter().find(|r| {
            let time_diff = (gh_event.created_at - r.created_at).num_seconds().abs();
            time_diff < TIMESTAMP_WINDOW_SECS
        }) {
            entry.railway = Some((*rw).clone());
            // Upgrade confidence if we have explicit mapping
            if project.railway.is_some() {
                entry.confidence = entry.confidence.max(Confidence::High);
            } else {
                entry.confidence = entry.confidence.max(Confidence::Low);
            }
        }

        correlated.push(entry);
    }

    correlated
}


8. Error Types

// crates/pulsos-core/src/error.rs

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PulsosError {
    // ── Network ──
    #[error("Network error reaching {platform}: {message}")]
    Network {
        platform: String,
        message: String,
        #[source]
        source: Option<reqwest::Error>,
    },

    #[error("Rate limit exceeded for {platform}. Resets at {reset_at}. Showing cached data.")]
    RateLimited {
        platform: String,
        reset_at: String,
        remaining: u32,
    },

    // ── Auth ──
    #[error("Authentication failed for {platform}: {reason}")]
    AuthFailed {
        platform: String,
        reason: String,
    },

    #[error("Token expired for {platform}. Run `pulsos auth {platform}` to refresh.")]
    TokenExpired { platform: String },

    #[error("Token has insufficient scopes for {platform}. Required: {required}, got: {actual}")]
    InsufficientScopes {
        platform: String,
        required: String,
        actual: String,
    },

    // ── API ──
    #[error("{platform} API returned {status}: {body}")]
    ApiError {
        platform: String,
        status: u16,
        body: String,
    },

    #[error("GraphQL error from {platform}: {message}")]
    GraphqlError {
        platform: String,
        message: String,
    },

    #[error("Failed to parse response from {platform}: {message}")]
    ParseError {
        platform: String,
        message: String,
    },

    // ── Config ──
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Config file not found. Run `pulsos` to set up.")]
    NoConfig,

    // ── Cache ──
    #[error("Cache error: {0}")]
    Cache(String),

    // ── Auth store ──
    #[error("Keyring error: {0}")]
    Keyring(String),

    // ── General ──
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl PulsosError {
    /// Convert to a user-friendly message with actionable guidance.
    pub fn user_message(&self) -> String {
        match self {
            Self::Network { platform, .. } => format!(
                "Could not reach {platform}.\n\n\
                 Possible causes:\n\
                 1. No internet connection\n\
                 2. {platform} is experiencing an outage\n\
                 3. Firewall or proxy blocking the request\n\n\
                 Showing cached data. Run `pulsos doctor` for diagnostics."
            ),
            Self::RateLimited { platform, reset_at, .. } => format!(
                "{platform} rate limit reached. Showing cached data.\n\
                 Resets at {reset_at}."
            ),
            Self::TokenExpired { platform } => format!(
                "{platform} token has expired.\n\
                 Run `pulsos auth {platform}` to authenticate again."
            ),
            _ => self.to_string(),
        }
    }
}


9. Config Types (TOML Serialization)

// crates/pulsos-core/src/config/types.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct PulsosConfig {
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub github: GitHubConfig,
    #[serde(default)]
    pub railway: RailwayConfig,
    #[serde(default)]
    pub vercel: VercelConfig,
    #[serde(default)]
    pub correlations: Vec<CorrelationConfig>,
    #[serde(default)]
    pub views: Vec<ViewConfig>,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub cache: CacheConfig,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default = "default_github_host")]
    pub github_host: String,
    #[serde(default)]
    pub token_detection: TokenDetectionConfig,
}

fn default_github_host() -> String { "github.com".to_string() }

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenDetectionConfig {
    #[serde(default = "default_true")]
    pub detect_gh_cli: bool,
    #[serde(default = "default_true")]
    pub detect_railway_cli: bool,
    #[serde(default = "default_true")]
    pub detect_vercel_cli: bool,
    #[serde(default = "default_true")]
    pub detect_env_vars: bool,
}

impl Default for TokenDetectionConfig {
    fn default() -> Self {
        Self {
            detect_gh_cli: true,
            detect_railway_cli: true,
            detect_vercel_cli: true,
            detect_env_vars: true,
        }
    }
}

fn default_true() -> bool { true }

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GitHubConfig {
    #[serde(default)]
    pub organizations: Vec<OrgConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OrgConfig {
    pub name: String,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub auto_discover: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RailwayConfig {
    #[serde(default)]
    pub workspaces: Vec<WorkspaceConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
    pub id: Option<String>,
    #[serde(default)]
    pub include_projects: Vec<String>,
    #[serde(default)]
    pub exclude_projects: Vec<String>,
    #[serde(default = "default_production")]
    pub default_environment: String,
}

fn default_production() -> String { "production".to_string() }

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct VercelConfig {
    #[serde(default)]
    pub teams: Vec<TeamConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TeamConfig {
    pub name: String,
    pub id: Option<String>,
    #[serde(default)]
    pub include_projects: Vec<String>,
    #[serde(default = "default_true")]
    pub include_preview_deployments: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CorrelationConfig {
    pub name: String,
    pub github_repo: Option<String>,
    pub railway_project: Option<String>,
    pub railway_workspace: Option<String>,
    pub railway_environment: Option<String>,
    pub vercel_project: Option<String>,
    pub vercel_team: Option<String>,
    #[serde(default)]
    pub branch_mapping: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ViewConfig {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub branch_filter: Option<String>,
    #[serde(default)]
    pub status_filter: Vec<String>,
    #[serde(default = "default_refresh")]
    pub refresh_interval: u64,
    #[serde(default)]
    pub vercel_include_previews: bool,
}

fn default_refresh() -> u64 { 5 }

#[derive(Debug, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_refresh")]
    pub refresh_interval: u64,
    #[serde(default = "default_fps")]
    pub fps: u64,
    #[serde(default = "default_dark")]
    pub theme: String,
    #[serde(default = "default_auto")]
    pub unicode: String,
    #[serde(default = "default_unified")]
    pub default_tab: String,
    #[serde(default = "default_true")]
    pub show_sparklines: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            refresh_interval: 5,
            fps: 10,
            theme: "dark".to_string(),
            unicode: "auto".to_string(),
            default_tab: "unified".to_string(),
            show_sparklines: true,
        }
    }
}

fn default_fps() -> u64 { 10 }
fn default_dark() -> String { "dark".to_string() }
fn default_auto() -> String { "auto".to_string() }
fn default_unified() -> String { "unified".to_string() }

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheConfig {
    pub directory: Option<String>,
    #[serde(default = "default_cache_mb")]
    pub max_size_mb: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self { directory: None, max_size_mb: 100 }
    }
}

fn default_cache_mb() -> u64 { 100 }


10. Railway GraphQL Queries

These .graphql files are used by graphql_client for compile-time type generation.

# crates/pulsos-core/src/platform/railway/queries/projects.graphql

query ProjectsQuery($teamId: String!) {
  projects(teamId: $teamId) {
    edges {
      node {
        id
        name
        description
        createdAt
        services {
          edges {
            node {
              id
              name
              icon
            }
          }
        }
        environments {
          edges {
            node {
              id
              name
            }
          }
        }
      }
    }
  }
}


# crates/pulsos-core/src/platform/railway/queries/deployments.graphql

query DeploymentsQuery($input: DeploymentListInput!, $first: Int) {
  deployments(input: $input, first: $first) {
    edges {
      node {
        id
        status
        createdAt
        staticUrl
      }
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}


# crates/pulsos-core/src/platform/railway/queries/service_instance.graphql

query ServiceInstanceQuery($serviceId: String!, $environmentId: String!) {
  serviceInstance(serviceId: $serviceId, environmentId: $environmentId) {
    id
    serviceName
    startCommand
    buildCommand
    rootDirectory
    healthcheckPath
    region
    numReplicas
    restartPolicyType
    restartPolicyMaxRetries
    latestDeployment {
      id
      status
      createdAt
    }
  }
}


11. Testing Strategy Overview

11.1 Test Categories

Category

What It Tests

Location

Runner

Unit tests

Individual functions, status mapping, health calculation

#[cfg(test)] in each module

cargo test

Integration tests

Full adapter → mock server → domain types

crates/pulsos-core/tests/

cargo test

Mock server tests

HTTP client + error handling + caching

Uses wiremock crate

cargo test

TUI render tests

Widget rendering against TestBackend

crates/pulsos-cli/tests/

cargo test

Config parsing

Round-trip TOML serialization

crates/pulsos-core/tests/

cargo test

11.2 Mock Server Pattern

// crates/pulsos-test/src/mock_server.rs

use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

pub async fn github_mock() -> MockServer {
    let server = MockServer::start().await;

    // Workflow runs endpoint
    Mock::given(method("GET"))
        .and(path("/repos/myorg/my-saas/actions/runs"))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(load_fixture("github/workflow_runs.json"))
                .append_header("X-RateLimit-Remaining", "4999")
                .append_header("X-RateLimit-Limit", "5000")
                .append_header("ETag", "\"abc123\"")
        )
        .mount(&server)
        .await;

    server
}


11.3 Fixture Files

Each fixture is a captured, sanitized API response stored as JSON. They live in crates/pulsos-test/src/fixtures/ and are loaded at test time. This ensures tests run without network access and produce deterministic results.

12. CI/CD Pipeline

# .github/workflows/ci.yml
name: CI
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace
      - run: cargo clippy --workspace -- -D warnings
      - run: cargo fmt --all -- --check

  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install cargo-audit cargo-deny
      - run: cargo audit
      - run: cargo deny check


# .github/workflows/release.yml
name: Release
on:
  push:
    tags: ['v*']

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact: pulsos-darwin-aarch64.tar.gz
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact: pulsos-darwin-x86_64.tar.gz
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact: pulsos-linux-x86_64-gnu.tar.gz
          - os: ubuntu-latest
            target: x86_64-unknown-linux-musl
            artifact: pulsos-linux-x86_64-musl.tar.gz
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            artifact: pulsos-linux-aarch64-gnu.tar.gz
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact: pulsos-windows-x86_64.zip
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }}
      - name: Package
        run: |
          cd target/${{ matrix.target }}/release
          tar czf ../../../${{ matrix.artifact }} pulsos
      - uses: softprops/action-gh-release@v2
        with:
          files: ${{ matrix.artifact }}


13. Implementation Decision Log

Decisions made during design that deviate from obvious choices:

Decision

Rationale

sled over SQLite for cache

Embedded Rust-native KV store. No C dependency, no FFI. Simpler than SQLite for our use case (pure key-value with TTL). Trade-off: no SQL queries, but we don't need them.

graphql_client over hand-rolled GraphQL

Compile-time type safety from .graphql files. Catches schema mismatches at build time instead of runtime.

keyring crate over custom encryption

OS-native credential storage is always more secure than application-level encryption. Fallback encrypted file covers headless servers.

reqwest with rustls-tls over openssl

No system OpenSSL dependency. Static linking. Smaller binary. Same security level.

Workspace with 3 crates over monolithic

Clean dependency boundaries. pulsos-core has zero CLI/TUI dependencies. pulsos-test provides shared fixtures without polluting the main crate.

thiserror for library, anyhow at CLI boundary

Library errors are typed and matchable. CLI wraps them with anyhow for easy .context() chaining.

30s minimum GitHub poll over 5s

Rate limit math. At 5s with 20 repos = 14,400 calls/hour = 3x budget. At 30s = 2,400 = 48% budget. With ETags, real cost is ~720.


