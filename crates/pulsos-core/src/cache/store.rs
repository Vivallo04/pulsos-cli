use crate::error::PulsosError;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub data: T,
    pub fetched_at: DateTime<Utc>,
    pub ttl_secs: u64,
    pub etag: Option<String>,
}

impl<T> CacheEntry<T> {
    pub fn new(data: T, ttl_secs: u64, etag: Option<String>) -> Self {
        Self {
            data,
            fetched_at: Utc::now(),
            ttl_secs,
            etag,
        }
    }

    pub fn is_fresh(&self) -> bool {
        let age = Utc::now() - self.fetched_at;
        age.num_seconds() < self.ttl_secs as i64
    }

    pub fn is_stale(&self) -> bool {
        !self.is_fresh() && !self.is_expired()
    }

    /// Expired = older than max staleness (TTL * 120 = 2 hours for 30s TTL).
    pub fn is_expired(&self) -> bool {
        let max_staleness = self.ttl_secs * 120;
        let age = Utc::now() - self.fetched_at;
        age.num_seconds() > max_staleness as i64
    }

    pub fn age(&self) -> Duration {
        let secs = (Utc::now() - self.fetched_at).num_seconds().max(0) as u64;
        Duration::from_secs(secs)
    }

    /// Human-readable age string: "3m ago", "2h ago", "just now"
    pub fn age_display(&self) -> String {
        let secs = self.age().as_secs();
        if secs < 5 {
            "just now".to_string()
        } else if secs < 60 {
            format!("{secs}s ago")
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else {
            format!("{}h ago", secs / 3600)
        }
    }
}

/// Persistent cache backed by sled.
pub struct CacheStore {
    db: sled::Db,
}

impl CacheStore {
    pub fn open(path: &Path) -> Result<Self, PulsosError> {
        let db = sled::open(path)
            .map_err(|e| PulsosError::Cache(format!("Failed to open cache: {e}")))?;
        Ok(Self { db })
    }

    /// Default cache path: ~/.cache/pulsos/
    pub fn open_default() -> Result<Self, PulsosError> {
        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| PulsosError::Cache("Could not determine cache directory".into()))?
            .join("pulsos");
        Self::open(&cache_dir)
    }

    pub fn get<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<CacheEntry<T>>, PulsosError> {
        match self.db.get(key.as_bytes()) {
            Ok(Some(bytes)) => {
                let entry: CacheEntry<T> = serde_json::from_slice(&bytes).map_err(|e| {
                    PulsosError::Cache(format!("Failed to deserialize cache entry '{key}': {e}"))
                })?;
                Ok(Some(entry))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(PulsosError::Cache(format!(
                "Failed to read cache key '{key}': {e}"
            ))),
        }
    }

    pub fn set<T: Serialize>(
        &self,
        key: &str,
        data: T,
        ttl_secs: u64,
        etag: Option<String>,
    ) -> Result<(), PulsosError> {
        let entry = CacheEntry::new(data, ttl_secs, etag);
        let bytes = serde_json::to_vec(&entry).map_err(|e| {
            PulsosError::Cache(format!("Failed to serialize cache entry '{key}': {e}"))
        })?;
        self.db
            .insert(key.as_bytes(), bytes)
            .map_err(|e| PulsosError::Cache(format!("Failed to write cache key '{key}': {e}")))?;
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<(), PulsosError> {
        self.db
            .remove(key.as_bytes())
            .map_err(|e| PulsosError::Cache(format!("Failed to delete cache key '{key}': {e}")))?;
        Ok(())
    }

    pub fn clear(&self) -> Result<(), PulsosError> {
        self.db
            .clear()
            .map_err(|e| PulsosError::Cache(format!("Failed to clear cache: {e}")))?;
        Ok(())
    }

    /// Number of entries in the cache.
    pub fn len(&self) -> usize {
        self.db.len()
    }

    pub fn is_empty(&self) -> bool {
        self.db.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache() -> CacheStore {
        let dir = tempfile::tempdir().unwrap();
        CacheStore::open(dir.path()).unwrap()
    }

    #[test]
    fn set_and_get() {
        let cache = temp_cache();
        cache.set("test:key", vec![1, 2, 3], 30, None).unwrap();
        let entry: CacheEntry<Vec<i32>> = cache.get("test:key").unwrap().unwrap();
        assert_eq!(entry.data, vec![1, 2, 3]);
        assert_eq!(entry.ttl_secs, 30);
        assert!(entry.is_fresh());
    }

    #[test]
    fn get_missing_key() {
        let cache = temp_cache();
        let entry: Option<CacheEntry<String>> = cache.get("nonexistent").unwrap();
        assert!(entry.is_none());
    }

    #[test]
    fn delete_key() {
        let cache = temp_cache();
        cache.set("to_delete", "value", 30, None).unwrap();
        assert!(cache.get::<String>("to_delete").unwrap().is_some());
        cache.delete("to_delete").unwrap();
        assert!(cache.get::<String>("to_delete").unwrap().is_none());
    }

    #[test]
    fn clear_all() {
        let cache = temp_cache();
        cache.set("key1", "a", 30, None).unwrap();
        cache.set("key2", "b", 30, None).unwrap();
        assert_eq!(cache.len(), 2);
        cache.clear().unwrap();
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_entry_with_etag() {
        let cache = temp_cache();
        cache
            .set("etag_test", "data", 30, Some("\"abc123\"".into()))
            .unwrap();
        let entry: CacheEntry<String> = cache.get("etag_test").unwrap().unwrap();
        assert_eq!(entry.etag, Some("\"abc123\"".into()));
    }

    #[test]
    fn freshness_of_new_entry() {
        let entry = CacheEntry::new("data", 30, None);
        assert!(entry.is_fresh());
        assert!(!entry.is_stale());
        assert!(!entry.is_expired());
    }

    #[test]
    fn age_display_just_now() {
        let entry = CacheEntry::new("data", 30, None);
        assert_eq!(entry.age_display(), "just now");
    }

    #[test]
    fn serialization_roundtrip() {
        let entry = CacheEntry::new(vec!["hello", "world"], 60, Some("etag".into()));
        let json = serde_json::to_vec(&entry).unwrap();
        let restored: CacheEntry<Vec<String>> = serde_json::from_slice(&json).unwrap();
        assert_eq!(restored.data, vec!["hello", "world"]);
        assert_eq!(restored.ttl_secs, 60);
        assert_eq!(restored.etag, Some("etag".into()));
    }
}
