use std::time::Duration;

use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::Result;

#[async_trait]
pub trait CacheBackend: Send + Sync {
    async fn set_raw(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()>;
    async fn get_raw(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn exists(&self, key: &str) -> Result<bool>;
    async fn clear(&self) -> Result<()>;
    async fn expire(&self, key: &str, ttl: Duration) -> Result<()>;
}

#[async_trait]
pub trait Cache: CacheBackend {
    async fn set<T>(&self, key: &str, value: &T, ttl: Duration) -> Result<()>
    where
        T: Serialize + Send + Sync,
    {
        let value_bytes = serde_json::to_vec(value)
            .map_err(|e| crate::Error::Config(format!("Failed to serialize value: {}", e)))?;

        self.set_raw(key, &value_bytes, ttl).await
    }

    async fn get<T>(&self, key: &str) -> Result<Option<T>>
    where
        T: DeserializeOwned + Send,
    {
        if let Some(raw) = self.get_raw(key).await? {
            let value = serde_json::from_slice::<T>(&raw)
                .map_err(|e| crate::Error::Config(format!("Failed to deserialize value: {}", e)))?;

            Ok(Some(value))
        } else {
            Ok(None)
        }
    }
}

/// 为所有实现了 CacheBackend 的类型自动实现 Cache
impl<T: CacheBackend> Cache for T {}

/// 缓存管理器，提供统一的缓存接口
#[derive(Clone)]
pub struct CacheManager<C: CacheBackend> {
    cache: C,
}

impl<C: CacheBackend> CacheManager<C> {
    pub fn new(cache: C) -> Self
    where
        C: CacheBackend + 'static,
    {
        Self { cache }
    }

    pub fn token_cache_key(key: &str) -> String {
        format!("token:{}", key)
    }

    pub async fn cache_token<T>(&self, key: &str, token: &T, ttl: Duration) -> Result<()>
    where
        T: Serialize + Send + Sync,
    {
        let key = Self::token_cache_key(key);
        self.cache.set(&key, token, ttl).await
    }

    pub async fn get_cached_token<T>(&self, key: &str) -> Result<Option<T>>
    where
        T: DeserializeOwned + Send,
    {
        let key = Self::token_cache_key(key);
        self.cache.get(&key).await
    }

    pub async fn remove_token_cache(&self, key: &str) -> Result<()> {
        let key = Self::token_cache_key(key);
        self.cache.delete(&key).await
    }

    pub async fn has_token_cache(&self, key: &str) -> Result<bool> {
        let key = Self::token_cache_key(key);
        self.cache.exists(&key).await
    }

    pub async fn set<T>(&self, key: &str, value: &T, ttl: Duration) -> Result<()>
    where
        T: Serialize + Send + Sync,
    {
        self.cache.set(key, value, ttl).await
    }

    pub async fn get<T>(&self, key: &str) -> Result<Option<T>>
    where
        T: DeserializeOwned + Send,
    {
        self.cache.get(key).await
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        self.cache.delete(key).await
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.cache.exists(key).await
    }

    pub async fn clear(&self) -> Result<()> {
        self.cache.clear().await
    }

    pub async fn expire(&self, key: &str, ttl: Duration) -> Result<()> {
        self.cache.expire(key, ttl).await
    }
}
