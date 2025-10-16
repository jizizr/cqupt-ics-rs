use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;

use cqupt_ics_core::{Error, Result, cache::CacheBackend};

#[inline]
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// 缓存条目头部大小：[过期时间戳(8字节)] + [创建时间戳(8字节)]
const HEADER_SIZE: usize = 16;

fn create_cache_entry(data: &[u8], ttl: Duration) -> Vec<u8> {
    let now = now_secs();
    let expires_at = now + ttl.as_secs();

    let mut entry = Vec::with_capacity(HEADER_SIZE + data.len());
    entry.extend_from_slice(&expires_at.to_le_bytes());
    entry.extend_from_slice(&now.to_le_bytes());
    entry.extend_from_slice(data);
    entry
}

fn parse_cache_entry(raw: &[u8]) -> Result<(bool, &[u8])> {
    if raw.len() < HEADER_SIZE {
        return Err(cqupt_ics_core::Error::Config(
            "Invalid cache entry format".to_string(),
        ));
    }

    let expires_at = u64::from_le_bytes(
        raw[0..8]
            .try_into()
            .map_err(|_| cqupt_ics_core::Error::Config("Invalid expires_at format".to_string()))?,
    );

    let is_expired = now_secs() > expires_at;
    let data = &raw[HEADER_SIZE..];

    Ok((is_expired, data))
}

#[derive(Debug, Clone)]
pub struct FileCache {
    cache_dir: PathBuf,
}

impl FileCache {
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir).map_err(|e| {
                cqupt_ics_core::Error::Config(format!("Failed to create cache directory: {}", e))
            })?;
        }

        Ok(Self { cache_dir })
    }

    pub fn with_default_dir(app_name: &str) -> Result<Self> {
        let cache_dir = Self::get_default_cache_dir(app_name)?;
        Self::new(cache_dir)
    }

    fn get_default_cache_dir(app_name: &str) -> Result<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            if let Some(home) = std::env::var_os("HOME") {
                Ok(PathBuf::from(home)
                    .join("Library")
                    .join("Caches")
                    .join(app_name))
            } else {
                Err(cqupt_ics_core::Error::Config(
                    "Cannot determine cache directory".to_string(),
                ))
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(cache_dir) = std::env::var_os("XDG_CACHE_HOME") {
                Ok(PathBuf::from(cache_dir).join(app_name))
            } else if let Some(home) = std::env::var_os("HOME") {
                Ok(PathBuf::from(home).join(".cache").join(app_name))
            } else {
                Err(cqupt_ics_core::Error::Config(
                    "Cannot determine cache directory".to_string(),
                ))
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
                Ok(PathBuf::from(local_app_data).join(app_name))
            } else {
                Err(cqupt_ics_core::Error::Config(
                    "Cannot determine cache directory".to_string(),
                ))
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Err(cqupt_ics_core::Error::Config(
                "Unsupported operating system for cache directory detection".to_string(),
            ))
        }
    }

    fn cache_file_path(&self, key: &str) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();

        self.cache_dir.join(format!("{:x}.json", hash))
    }
}

#[async_trait]
impl CacheBackend for FileCache {
    async fn set_raw(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        if let Some(parent) = self.cache_dir.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Internal(format!("Failed to create cache directory: {}", e)))?;
        }

        let entry_with_header = create_cache_entry(value, ttl);

        let file_path = self.cache_file_path(key);
        tokio::fs::write(file_path, entry_with_header)
            .await
            .map_err(|e| Error::Internal(format!("Failed to write cache file: {}", e)))?;
        Ok(())
    }

    async fn get_raw(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let file_path = self.cache_file_path(key);

        if !file_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read(file_path).await.map_err(|e| {
            cqupt_ics_core::Error::Config(format!("Failed to read cache file: {}", e))
        })?;

        match parse_cache_entry(&content) {
            Ok((is_expired, data)) => {
                if is_expired {
                    let _ = self.delete(key).await;
                    Ok(None)
                } else {
                    Ok(Some(data.to_vec()))
                }
            }
            Err(_) => {
                let _ = self.delete(key).await;
                Ok(None)
            }
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let file_path = self.cache_file_path(key);
        if file_path.exists() {
            tokio::fs::remove_file(file_path).await.map_err(|e| {
                cqupt_ics_core::Error::Config(format!("Failed to delete cache file: {}", e))
            })?;
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let file_path = self.cache_file_path(key);

        if !file_path.exists() {
            return Ok(false);
        }

        let content = tokio::fs::read(&file_path).await.map_err(|e| {
            cqupt_ics_core::Error::Config(format!("Failed to read cache file: {}", e))
        })?;

        match parse_cache_entry(&content) {
            Ok((is_expired, _data)) => {
                if is_expired {
                    let _ = tokio::fs::remove_file(file_path).await;
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(file_path).await;
                Ok(false)
            }
        }
    }

    async fn clear(&self) -> Result<()> {
        let mut entries = tokio::fs::read_dir(&self.cache_dir).await.map_err(|e| {
            cqupt_ics_core::Error::Config(format!("Failed to read cache directory: {}", e))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            cqupt_ics_core::Error::Config(format!("Failed to read directory entry: {}", e))
        })? {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                let _ = tokio::fs::remove_file(path).await;
            }
        }

        Ok(())
    }

    async fn expire(&self, key: &str, ttl: Duration) -> Result<()> {
        let file_path = self.cache_file_path(key);

        if !file_path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read(&file_path).await.map_err(|e| {
            cqupt_ics_core::Error::Config(format!("Failed to read cache file: {}", e))
        })?;

        match parse_cache_entry(&content) {
            Ok((_is_expired, data)) => {
                let new_entry = create_cache_entry(data, ttl);

                tokio::fs::write(file_path, new_entry).await.map_err(|e| {
                    cqupt_ics_core::Error::Config(format!("Failed to write cache file: {}", e))
                })?;

                Ok(())
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(file_path).await;
                Ok(())
            }
        }
    }
}
