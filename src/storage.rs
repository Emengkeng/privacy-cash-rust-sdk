//! Local storage for caching UTXOs and offsets

use crate::error::{PrivacyCashError, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Storage backend trait
pub trait StorageBackend: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
    fn remove(&self, key: &str);
    fn clear(&self);
}

/// File-based storage implementation
pub struct FileStorage {
    cache_dir: PathBuf,
    cache: RwLock<HashMap<String, String>>,
}

impl FileStorage {
    /// Create a new file storage in the specified directory
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        // Create directory if it doesn't exist
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)
                .map_err(|e| PrivacyCashError::StorageError(format!("Failed to create cache dir: {}", e)))?;
        }

        let storage = Self {
            cache_dir,
            cache: RwLock::new(HashMap::new()),
        };

        // Load existing cache files
        storage.load_cache()?;

        Ok(storage)
    }

    /// Create storage in the default cache directory
    pub fn default_cache() -> Result<Self> {
        let cache_dir = std::env::current_dir()
            .map_err(|e| PrivacyCashError::StorageError(format!("Failed to get current dir: {}", e)))?
            .join("cache");

        Self::new(cache_dir)
    }

    /// Load all cached values from disk
    fn load_cache(&self) -> Result<()> {
        if !self.cache_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.cache_dir)
            .map_err(|e| PrivacyCashError::StorageError(format!("Failed to read cache dir: {}", e)))?;

        let mut cache = self.cache.write();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(key) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(value) = fs::read_to_string(&path) {
                        cache.insert(key.to_string(), value);
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the file path for a key
    fn key_path(&self, key: &str) -> PathBuf {
        // Sanitize key to be safe for filesystem
        let safe_key = key.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        self.cache_dir.join(safe_key)
    }
}

impl StorageBackend for FileStorage {
    fn get(&self, key: &str) -> Option<String> {
        let cache = self.cache.read();
        cache.get(key).cloned()
    }

    fn set(&self, key: &str, value: &str) {
        // Update in-memory cache
        {
            let mut cache = self.cache.write();
            cache.insert(key.to_string(), value.to_string());
        }

        // Persist to disk (ignore errors)
        let path = self.key_path(key);
        let _ = fs::write(path, value);
    }

    fn remove(&self, key: &str) {
        // Remove from in-memory cache
        {
            let mut cache = self.cache.write();
            cache.remove(key);
        }

        // Remove from disk
        let path = self.key_path(key);
        let _ = fs::remove_file(path);
    }

    fn clear(&self) {
        // Clear in-memory cache
        {
            let mut cache = self.cache.write();
            cache.clear();
        }

        // Clear disk cache
        if self.cache_dir.exists() {
            let _ = fs::remove_dir_all(&self.cache_dir);
            let _ = fs::create_dir_all(&self.cache_dir);
        }
    }
}

/// In-memory storage (for testing or ephemeral use)
pub struct MemoryStorage {
    data: RwLock<HashMap<String, String>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageBackend for MemoryStorage {
    fn get(&self, key: &str) -> Option<String> {
        self.data.read().get(key).cloned()
    }

    fn set(&self, key: &str, value: &str) {
        self.data.write().insert(key.to_string(), value.to_string());
    }

    fn remove(&self, key: &str) {
        self.data.write().remove(key);
    }

    fn clear(&self) {
        self.data.write().clear();
    }
}

/// Storage wrapper for the SDK
pub struct Storage {
    backend: Box<dyn StorageBackend>,
}

impl Storage {
    /// Create storage with file backend
    pub fn file(cache_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            backend: Box::new(FileStorage::new(cache_dir)?),
        })
    }

    /// Create storage with default file backend
    pub fn default_file() -> Result<Self> {
        Ok(Self {
            backend: Box::new(FileStorage::default_cache()?),
        })
    }

    /// Create storage with memory backend
    pub fn memory() -> Self {
        Self {
            backend: Box::new(MemoryStorage::new()),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.backend.get(key)
    }

    pub fn set(&self, key: &str, value: &str) {
        self.backend.set(key, value);
    }

    pub fn remove(&self, key: &str) {
        self.backend.remove(key);
    }

    pub fn clear(&self) {
        self.backend.clear();
    }
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage").finish()
    }
}
