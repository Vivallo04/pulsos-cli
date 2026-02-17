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

// ── In-Memory Implementation (for tests) ──

use std::collections::HashMap;
use std::sync::Mutex;

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
}
