use crate::auth::PlatformKind;
use crate::error::PulsosError;
use secrecy::SecretString;

/// Abstraction over credential storage backends.
///
/// This trait enables testability by allowing an in-memory store to
/// replace the OS keyring in tests.
pub trait CredentialStore: Send + Sync {
    /// Retrieve the stored token for a platform. Returns `None` if no token is stored.
    fn get(&self, platform: &PlatformKind) -> Result<Option<SecretString>, PulsosError>;

    /// Store a token for a platform, overwriting any existing value.
    fn set(&self, platform: &PlatformKind, token: &str) -> Result<(), PulsosError>;

    /// Delete the stored token for a platform. No-op if no token is stored.
    fn delete(&self, platform: &PlatformKind) -> Result<(), PulsosError>;
}

// ── OS Keyring Implementation ──

const KEYRING_SERVICE: &str = "pulsos";

/// Credential store backed by the OS keyring (macOS Keychain, Windows Credential Manager,
/// Linux Secret Service / libsecret).
pub struct KeyringStore;

impl KeyringStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(&self, platform: &PlatformKind) -> Result<keyring::Entry, PulsosError> {
        keyring::Entry::new(KEYRING_SERVICE, platform.keyring_username()).map_err(|e| {
            PulsosError::Keyring(format!(
                "Failed to access keyring for {}: {}",
                platform.display_name(),
                e
            ))
        })
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for KeyringStore {
    fn get(&self, platform: &PlatformKind) -> Result<Option<SecretString>, PulsosError> {
        let entry = self.entry(platform)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(SecretString::new(password))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(PulsosError::Keyring(format!(
                "Failed to read {} token from keyring: {}",
                platform.display_name(),
                e
            ))),
        }
    }

    fn set(&self, platform: &PlatformKind, token: &str) -> Result<(), PulsosError> {
        let entry = self.entry(platform)?;
        entry.set_password(token).map_err(|e| {
            PulsosError::Keyring(format!(
                "Failed to store {} token in keyring: {}",
                platform.display_name(),
                e
            ))
        })
    }

    fn delete(&self, platform: &PlatformKind) -> Result<(), PulsosError> {
        let entry = self.entry(platform)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already deleted, no-op
            Err(e) => Err(PulsosError::Keyring(format!(
                "Failed to delete {} token from keyring: {}",
                platform.display_name(),
                e
            ))),
        }
    }
}

// ── File-based Implementation ──

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// TOML structure for the credentials file.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct CredentialsFile {
    #[serde(default)]
    tokens: HashMap<String, String>,
}

/// Credential store backed by a plain TOML file at
/// `~/.config/pulsos/credentials.toml` with mode `0600` on Unix.
///
/// No encryption is applied — this is a fallback for environments
/// where the OS keyring is unavailable.
pub struct FileCredentialStore {
    path: PathBuf,
}

impl FileCredentialStore {
    pub fn new() -> Result<Self, PulsosError> {
        Ok(Self {
            path: Self::default_path()?,
        })
    }

    pub fn default_path() -> Result<PathBuf, PulsosError> {
        dirs::config_dir()
            .map(|d| d.join("pulsos").join("credentials.toml"))
            .ok_or_else(|| PulsosError::Config("Could not determine config directory".into()))
    }

    fn read(&self) -> Result<CredentialsFile, PulsosError> {
        if !self.path.exists() {
            return Ok(CredentialsFile::default());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| PulsosError::Keyring(format!("Failed to read credentials file: {e}")))?;
        toml::from_str(&content)
            .map_err(|e| PulsosError::Keyring(format!("Failed to parse credentials file: {e}")))
    }

    fn write(&self, creds: &CredentialsFile) -> Result<(), PulsosError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                PulsosError::Keyring(format!("Failed to create credentials directory: {e}"))
            })?;
        }

        let content = toml::to_string_pretty(creds)
            .map_err(|e| PulsosError::Keyring(format!("Failed to serialize credentials: {e}")))?;

        std::fs::write(&self.path, content)
            .map_err(|e| PulsosError::Keyring(format!("Failed to write credentials file: {e}")))?;

        // Restrict permissions to owner-only on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600)).map_err(
                |e| {
                    PulsosError::Keyring(format!("Failed to set credentials file permissions: {e}"))
                },
            )?;
        }

        Ok(())
    }
}

impl Default for FileCredentialStore {
    fn default() -> Self {
        Self {
            path: Self::default_path().unwrap_or_else(|_| PathBuf::from("credentials.toml")),
        }
    }
}

impl CredentialStore for FileCredentialStore {
    fn get(&self, platform: &PlatformKind) -> Result<Option<SecretString>, PulsosError> {
        let creds = self.read()?;
        Ok(creds
            .tokens
            .get(platform.keyring_username())
            .map(|t| SecretString::new(t.clone())))
    }

    fn set(&self, platform: &PlatformKind, token: &str) -> Result<(), PulsosError> {
        let mut creds = self.read()?;
        creds
            .tokens
            .insert(platform.keyring_username().to_string(), token.to_string());
        self.write(&creds)
    }

    fn delete(&self, platform: &PlatformKind) -> Result<(), PulsosError> {
        let mut creds = self.read()?;
        creds.tokens.remove(platform.keyring_username());
        self.write(&creds)
    }
}

// ── Fallback Store (keyring → file) ──

/// Credential store that tries the OS keyring first, then falls back to
/// the file-based store. This ensures credentials are stored and retrieved
/// even when the OS keyring is unavailable (CI, headless servers, WSL, etc.).
pub struct FallbackStore {
    keyring: KeyringStore,
    file: FileCredentialStore,
    session: Mutex<HashMap<String, String>>,
}

impl FallbackStore {
    pub fn new() -> Result<Self, PulsosError> {
        Ok(Self {
            keyring: KeyringStore::new(),
            file: FileCredentialStore::new()?,
            session: Mutex::new(HashMap::new()),
        })
    }
}

impl CredentialStore for FallbackStore {
    fn get(&self, platform: &PlatformKind) -> Result<Option<SecretString>, PulsosError> {
        // Prefer values set during this process to make first-run onboarding reliable.
        if let Ok(session) = self.session.lock() {
            if let Some(token) = session.get(platform.keyring_username()) {
                return Ok(Some(SecretString::new(token.clone())));
            }
        }

        // Prefer keyring; fall back to file when keyring has no entry or errors.
        match self.keyring.get(platform) {
            Ok(Some(token)) => return Ok(Some(token)),
            _ => {}
        }
        self.file.get(platform)
    }

    fn set(&self, platform: &PlatformKind, token: &str) -> Result<(), PulsosError> {
        // Write to keyring first (best security), but also persist to the fallback
        // file store so fresh processes can always resolve credentials in environments
        // where keyring read behavior is inconsistent.
        let keyring_result = self.keyring.set(platform, token);
        if let Err(e) = &keyring_result {
            tracing::info!(
                platform = platform.display_name(),
                error = %e,
                "OS keyring unavailable, falling back to credentials file"
            );
        }
        let file_result = self.file.set(platform, token);
        if let Err(e) = &file_result {
            tracing::debug!(
                platform = platform.display_name(),
                error = %e,
                "Failed to persist token to fallback credentials file"
            );
        }

        let set_result = if keyring_result.is_ok() || file_result.is_ok() {
            Ok(())
        } else {
            file_result
        };

        if set_result.is_ok() {
            if let Ok(mut session) = self.session.lock() {
                session.insert(platform.keyring_username().to_string(), token.to_string());
            }
        }

        set_result
    }

    fn delete(&self, platform: &PlatformKind) -> Result<(), PulsosError> {
        if let Ok(mut session) = self.session.lock() {
            session.remove(platform.keyring_username());
        }

        // Best-effort deletion from both stores.
        let _ = self.keyring.delete(platform);
        let _ = self.file.delete(platform);
        Ok(())
    }
}

// ── In-Memory Implementation (for tests) ──

/// In-memory credential store for testing. Thread-safe via Mutex.
pub struct InMemoryStore {
    tokens: Mutex<HashMap<String, String>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for InMemoryStore {
    fn get(&self, platform: &PlatformKind) -> Result<Option<SecretString>, PulsosError> {
        let tokens = self
            .tokens
            .lock()
            .map_err(|e| PulsosError::Keyring(format!("InMemoryStore lock poisoned: {}", e)))?;
        Ok(tokens
            .get(platform.keyring_username())
            .map(|t| SecretString::new(t.clone())))
    }

    fn set(&self, platform: &PlatformKind, token: &str) -> Result<(), PulsosError> {
        let mut tokens = self
            .tokens
            .lock()
            .map_err(|e| PulsosError::Keyring(format!("InMemoryStore lock poisoned: {}", e)))?;
        tokens.insert(platform.keyring_username().to_string(), token.to_string());
        Ok(())
    }

    fn delete(&self, platform: &PlatformKind) -> Result<(), PulsosError> {
        let mut tokens = self
            .tokens
            .lock()
            .map_err(|e| PulsosError::Keyring(format!("InMemoryStore lock poisoned: {}", e)))?;
        tokens.remove(platform.keyring_username());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    /// Helper to extract the exposed secret for test assertions.
    fn exposed(secret: Option<SecretString>) -> Option<String> {
        secret.map(|s| s.expose_secret().to_string())
    }

    #[test]
    fn in_memory_store_get_missing_returns_none() {
        let store = InMemoryStore::new();
        let result = store.get(&PlatformKind::GitHub).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn in_memory_store_set_and_get() {
        let store = InMemoryStore::new();
        store.set(&PlatformKind::GitHub, "ghp_test123").unwrap();
        let result = exposed(store.get(&PlatformKind::GitHub).unwrap());
        assert_eq!(result, Some("ghp_test123".to_string()));
    }

    #[test]
    fn in_memory_store_overwrite() {
        let store = InMemoryStore::new();
        store.set(&PlatformKind::Railway, "token1").unwrap();
        store.set(&PlatformKind::Railway, "token2").unwrap();
        let result = exposed(store.get(&PlatformKind::Railway).unwrap());
        assert_eq!(result, Some("token2".to_string()));
    }

    #[test]
    fn in_memory_store_delete() {
        let store = InMemoryStore::new();
        store.set(&PlatformKind::Vercel, "vc_token").unwrap();
        store.delete(&PlatformKind::Vercel).unwrap();
        let result = store.get(&PlatformKind::Vercel).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn in_memory_store_delete_missing_is_noop() {
        let store = InMemoryStore::new();
        // Should not error
        store.delete(&PlatformKind::GitHub).unwrap();
    }

    #[test]
    fn in_memory_store_platforms_are_isolated() {
        let store = InMemoryStore::new();
        store.set(&PlatformKind::GitHub, "gh_token").unwrap();
        store.set(&PlatformKind::Railway, "rw_token").unwrap();

        assert_eq!(
            exposed(store.get(&PlatformKind::GitHub).unwrap()),
            Some("gh_token".to_string())
        );
        assert_eq!(
            exposed(store.get(&PlatformKind::Railway).unwrap()),
            Some("rw_token".to_string())
        );
        assert!(store.get(&PlatformKind::Vercel).unwrap().is_none());
    }

    // ── FileCredentialStore tests ──

    fn file_store_at(dir: &tempfile::TempDir) -> FileCredentialStore {
        FileCredentialStore {
            path: dir.path().join("credentials.toml"),
        }
    }

    #[test]
    fn file_store_get_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_store_at(&dir);
        assert!(store.get(&PlatformKind::GitHub).unwrap().is_none());
    }

    #[test]
    fn file_store_set_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_store_at(&dir);
        store.set(&PlatformKind::GitHub, "ghp_test").unwrap();
        let result = exposed(store.get(&PlatformKind::GitHub).unwrap());
        assert_eq!(result, Some("ghp_test".to_string()));
    }

    #[test]
    fn file_store_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_store_at(&dir);
        store.set(&PlatformKind::Railway, "token1").unwrap();
        store.set(&PlatformKind::Railway, "token2").unwrap();
        let result = exposed(store.get(&PlatformKind::Railway).unwrap());
        assert_eq!(result, Some("token2".to_string()));
    }

    #[test]
    fn file_store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_store_at(&dir);
        store.set(&PlatformKind::Vercel, "vc_token").unwrap();
        store.delete(&PlatformKind::Vercel).unwrap();
        assert!(store.get(&PlatformKind::Vercel).unwrap().is_none());
    }

    #[test]
    fn file_store_platforms_isolated() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_store_at(&dir);
        store.set(&PlatformKind::GitHub, "gh_tok").unwrap();
        store.set(&PlatformKind::Railway, "rw_tok").unwrap();
        assert_eq!(
            exposed(store.get(&PlatformKind::GitHub).unwrap()),
            Some("gh_tok".to_string())
        );
        assert_eq!(
            exposed(store.get(&PlatformKind::Railway).unwrap()),
            Some("rw_tok".to_string())
        );
        assert!(store.get(&PlatformKind::Vercel).unwrap().is_none());
    }
}
