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
    AuthFailed { platform: String, reason: String },

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
    GraphqlError { platform: String, message: String },

    #[error("Failed to parse response from {platform}: {message}")]
    ParseError { platform: String, message: String },

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
    #[error(transparent)]
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
            Self::RateLimited {
                platform, reset_at, ..
            } => format!(
                "{platform} rate limit reached. Showing cached data.\n\
                 Resets at {reset_at}."
            ),
            Self::TokenExpired { platform } => format!(
                "{platform} token has expired.\n\
                 Run `pulsos auth {platform}` to authenticate again."
            ),
            Self::AuthFailed { platform, reason } => format!(
                "Authentication failed for {platform}: {reason}\n\
                 Run `pulsos auth {platform}` to re-authenticate."
            ),
            Self::InsufficientScopes {
                platform,
                required,
                actual,
            } => format!(
                "{platform} token has insufficient scopes.\n\
                 Required: {required}\n\
                 Current:  {actual}\n\
                 Create a new token with the correct scopes."
            ),
            Self::NoConfig => "No configuration found. Run `pulsos` to set up.".to_string(),

            Self::ApiError {
                platform,
                status,
                body,
            } => {
                let hint = match *status {
                    400 => "The request was malformed. This may be a bug in pulsos.".to_string(),
                    403 => format!(
                        "Access denied. Check your {platform} token permissions."
                    ),
                    404 => "The resource was not found. It may have been deleted or you may lack access.".to_string(),
                    500..=599 => format!(
                        "{platform} is experiencing server issues. Try again later."
                    ),
                    _ => format!("Unexpected HTTP {status} from {platform}."),
                };
                let body_preview = if body.len() > 200 {
                    format!("{}...", &body[..200])
                } else {
                    body.clone()
                };
                format!(
                    "{platform} API error (HTTP {status}).\n\n\
                     {hint}\n\n\
                     Response: {body_preview}\n\n\
                     Run `pulsos doctor` to check your configuration."
                )
            }

            Self::GraphqlError { platform, message } => format!(
                "{platform} GraphQL query failed: {message}\n\n\
                 This may indicate:\n\
                 1. An API schema change on {platform}\n\
                 2. Insufficient permissions for this query\n\
                 3. A temporary {platform} service issue\n\n\
                 Run `pulsos doctor` to verify your authentication."
            ),

            Self::ParseError { platform, message } => format!(
                "Failed to parse {platform} response: {message}\n\n\
                 This usually means the API returned an unexpected format.\n\
                 If this persists, please report it at:\n\
                   https://github.com/lambdahq/pulsos/issues"
            ),

            Self::Config(detail) => format!(
                "Configuration error: {detail}\n\n\
                 Check your config file at ~/.config/pulsos/config.toml\n\
                 Run `pulsos repos sync` to regenerate the configuration."
            ),

            Self::Cache(detail) => format!(
                "Cache error: {detail}\n\n\
                 Try clearing the cache:\n\
                   rm -rf ~/.cache/pulsos/\n\n\
                 Pulsos will rebuild the cache automatically on the next run."
            ),

            Self::Keyring(detail) => format!(
                "Credential store error: {detail}\n\n\
                 Your OS keyring may be locked or unavailable.\n\
                 Possible fixes:\n\
                 1. Unlock your keyring (e.g., login keychain on macOS)\n\
                 2. Use environment variables instead: set GH_TOKEN, RAILWAY_TOKEN, VERCEL_TOKEN\n\
                 3. Re-authenticate: `pulsos auth github`"
            ),

            Self::Other(e) => format!(
                "Unexpected error: {e}\n\n\
                 If this persists, please report it at:\n\
                   https://github.com/lambdahq/pulsos/issues\n\n\
                 Include the output of `pulsos doctor` in your report."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_error_display() {
        let err = PulsosError::Network {
            platform: "GitHub".into(),
            message: "connection refused".into(),
            source: None,
        };
        assert!(err.to_string().contains("GitHub"));
        assert!(err.to_string().contains("connection refused"));
    }

    #[test]
    fn network_error_user_message() {
        let err = PulsosError::Network {
            platform: "GitHub".into(),
            message: "timeout".into(),
            source: None,
        };
        let msg = err.user_message();
        assert!(msg.contains("Could not reach GitHub"));
        assert!(msg.contains("No internet connection"));
        assert!(msg.contains("pulsos doctor"));
    }

    #[test]
    fn rate_limited_user_message() {
        let err = PulsosError::RateLimited {
            platform: "GitHub".into(),
            reset_at: "14:32".into(),
            remaining: 0,
        };
        let msg = err.user_message();
        assert!(msg.contains("rate limit reached"));
        assert!(msg.contains("14:32"));
    }

    #[test]
    fn token_expired_user_message() {
        let err = PulsosError::TokenExpired {
            platform: "Railway".into(),
        };
        let msg = err.user_message();
        assert!(msg.contains("Railway"));
        assert!(msg.contains("pulsos auth Railway"));
    }

    #[test]
    fn no_config_user_message() {
        let err = PulsosError::NoConfig;
        let msg = err.user_message();
        assert!(msg.contains("Run `pulsos`"));
    }

    #[test]
    fn from_anyhow_error() {
        let anyhow_err = anyhow::anyhow!("something went wrong");
        let err: PulsosError = anyhow_err.into();
        assert!(matches!(err, PulsosError::Other(_)));
        assert!(err.to_string().contains("something went wrong"));
    }

    #[test]
    fn api_error_403_user_message() {
        let err = PulsosError::ApiError {
            platform: "GitHub".into(),
            status: 403,
            body: "Forbidden".into(),
        };
        let msg = err.user_message();
        assert!(msg.contains("HTTP 403"));
        assert!(msg.contains("token permissions"));
        assert!(msg.contains("pulsos doctor"));
    }

    #[test]
    fn api_error_500_user_message() {
        let err = PulsosError::ApiError {
            platform: "Vercel".into(),
            status: 500,
            body: "Internal Server Error".into(),
        };
        let msg = err.user_message();
        assert!(msg.contains("HTTP 500"));
        assert!(msg.contains("server issues"));
    }

    #[test]
    fn api_error_truncates_long_body() {
        let err = PulsosError::ApiError {
            platform: "Vercel".into(),
            status: 500,
            body: "x".repeat(500),
        };
        let msg = err.user_message();
        assert!(msg.contains("..."));
        // The body preview should be 200 chars + "..."
        assert!(!msg.contains(&"x".repeat(300)));
    }

    #[test]
    fn graphql_error_user_message() {
        let err = PulsosError::GraphqlError {
            platform: "Railway".into(),
            message: "field not found".into(),
        };
        let msg = err.user_message();
        assert!(msg.contains("Railway"));
        assert!(msg.contains("GraphQL"));
        assert!(msg.contains("pulsos doctor"));
    }

    #[test]
    fn parse_error_user_message() {
        let err = PulsosError::ParseError {
            platform: "GitHub".into(),
            message: "expected object".into(),
        };
        let msg = err.user_message();
        assert!(msg.contains("unexpected format"));
        assert!(msg.contains("github.com/lambdahq/pulsos/issues"));
    }

    #[test]
    fn config_error_user_message() {
        let err = PulsosError::Config("invalid TOML".into());
        let msg = err.user_message();
        assert!(msg.contains("invalid TOML"));
        assert!(msg.contains("config.toml"));
        assert!(msg.contains("pulsos repos sync"));
    }

    #[test]
    fn cache_error_user_message() {
        let err = PulsosError::Cache("corrupted".into());
        let msg = err.user_message();
        assert!(msg.contains("corrupted"));
        assert!(msg.contains("rm -rf"));
    }

    #[test]
    fn keyring_error_user_message() {
        let err = PulsosError::Keyring("access denied".into());
        let msg = err.user_message();
        assert!(msg.contains("keyring"));
        assert!(msg.contains("GH_TOKEN"));
        assert!(msg.contains("pulsos auth"));
    }

    #[test]
    fn other_error_user_message() {
        let err = PulsosError::Other(anyhow::anyhow!("weird failure"));
        let msg = err.user_message();
        assert!(msg.contains("weird failure"));
        assert!(msg.contains("pulsos doctor"));
    }
}
