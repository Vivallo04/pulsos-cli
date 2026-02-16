pub mod github;
pub mod railway;
pub mod vercel;

use crate::domain::deployment::DeploymentEvent;
use crate::error::PulsosError;
use chrono::{DateTime, Utc};

/// Every platform adapter implements this trait.
/// The trait uses domain types exclusively — no wire types leak out.
pub trait PlatformAdapter: Send + Sync {
    /// Fetch the latest deployment events for all tracked resources.
    /// Returns events sorted by created_at descending.
    fn fetch_events(
        &self,
        tracked: &[TrackedResource],
    ) -> impl std::future::Future<Output = Result<Vec<DeploymentEvent>, PulsosError>> + Send;

    /// Discover all available resources (repos, projects, etc.)
    /// Used during first-run wizard and `repos sync`.
    fn discover(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<DiscoveredResource>, PulsosError>> + Send;

    /// Validate that the stored token is still valid and has correct scopes.
    fn validate_auth(
        &self,
    ) -> impl std::future::Future<Output = Result<AuthStatus, PulsosError>> + Send;

    /// Return the current rate limit status.
    fn rate_limit_status(
        &self,
    ) -> impl std::future::Future<Output = Result<RateLimitInfo, PulsosError>> + Send;
}

/// A resource the user has chosen to track.
#[derive(Debug, Clone)]
pub struct TrackedResource {
    /// repo full_name, project ID, etc.
    pub platform_id: String,
    pub display_name: String,
    /// org, workspace, team name
    pub group: Option<String>,
}

/// A resource discovered during scanning.
#[derive(Debug, Clone)]
pub struct DiscoveredResource {
    pub platform_id: String,
    pub display_name: String,
    pub group: String,
    /// "organization", "workspace", "team"
    pub group_type: String,
    pub archived: bool,
    pub disabled: bool,
}

/// Auth validation result.
#[derive(Debug)]
pub struct AuthStatus {
    pub valid: bool,
    /// "@vivallo", "v@lambda.co", "lambda-team"
    pub identity: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    /// "Token has unnecessary write scope"
    pub warnings: Vec<String>,
}

/// Rate limit info for display in `doctor`.
#[derive(Debug)]
pub struct RateLimitInfo {
    pub limit: u32,
    pub remaining: u32,
    pub resets_at: DateTime<Utc>,
    pub percentage_used: f32,
}
