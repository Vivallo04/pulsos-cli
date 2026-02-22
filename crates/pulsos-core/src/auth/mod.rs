pub mod credential_store;
pub mod detect;
pub mod resolve;
pub mod validate;

use std::fmt;

/// Identifies which platform we're authenticating with.
/// Separate from `domain::deployment::Platform` which is for domain modeling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PlatformKind {
    GitHub,
    Railway,
    Vercel,
}

impl PlatformKind {
    /// All supported platforms.
    pub const ALL: [PlatformKind; 3] = [
        PlatformKind::GitHub,
        PlatformKind::Railway,
        PlatformKind::Vercel,
    ];

    /// Keyring username for this platform's token.
    pub fn keyring_username(&self) -> &'static str {
        match self {
            Self::GitHub => "github_token",
            Self::Railway => "railway_token",
            Self::Vercel => "vercel_token",
        }
    }

    /// Environment variable names to check (in priority order).
    pub fn env_var_names(&self) -> &'static [&'static str] {
        match self {
            Self::GitHub => &["GH_TOKEN", "GITHUB_TOKEN"],
            Self::Railway => &["RAILWAY_TOKEN", "RAILWAY_API_TOKEN"],
            Self::Vercel => &["VERCEL_TOKEN"],
        }
    }

    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::Railway => "Railway",
            Self::Vercel => "Vercel",
        }
    }

    /// CLI subcommand name (lowercase).
    pub fn cli_name(&self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Railway => "railway",
            Self::Vercel => "vercel",
        }
    }
}

impl fmt::Display for PlatformKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_kind_env_vars() {
        assert_eq!(
            PlatformKind::GitHub.env_var_names(),
            &["GH_TOKEN", "GITHUB_TOKEN"]
        );
        assert_eq!(
            PlatformKind::Railway.env_var_names(),
            &["RAILWAY_TOKEN", "RAILWAY_API_TOKEN"]
        );
        assert_eq!(PlatformKind::Vercel.env_var_names(), &["VERCEL_TOKEN"]);
    }

    #[test]
    fn platform_kind_keyring_username() {
        assert_eq!(PlatformKind::GitHub.keyring_username(), "github_token");
        assert_eq!(PlatformKind::Railway.keyring_username(), "railway_token");
        assert_eq!(PlatformKind::Vercel.keyring_username(), "vercel_token");
    }

    #[test]
    fn platform_kind_display() {
        assert_eq!(PlatformKind::GitHub.to_string(), "GitHub");
        assert_eq!(PlatformKind::Railway.to_string(), "Railway");
        assert_eq!(PlatformKind::Vercel.to_string(), "Vercel");
    }

    #[test]
    fn all_platforms() {
        assert_eq!(PlatformKind::ALL.len(), 3);
    }
}
