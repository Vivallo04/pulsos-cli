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
            _ => self.to_string(),
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
}
