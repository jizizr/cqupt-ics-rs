use std::time::Duration;

use async_trait::async_trait;

use cqupt_ics_core::{Result, cache::CacheBackend};

/// Redis 缓存实现
#[derive(Clone)]
pub struct RedisCache {
    connection: redis::aio::ConnectionManager,
    prefix: String,
}

impl RedisCache {
    /// 创建新的 Redis 缓存实例
    pub fn new(prefix: String, connection: redis::aio::ConnectionManager) -> Self {
        Self { connection, prefix }
    }

    /// 构建带前缀的键
    fn build_key(&self, key: &str) -> String {
        format!("{}:{}", self.prefix, key)
    }
}

#[async_trait]
impl CacheBackend for RedisCache {
    async fn set_raw(&self, key: &str, value: &[u8], ttl: Duration) -> Result<()> {
        use redis::AsyncCommands;

        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        conn.set_ex::<_, _, ()>(&full_key, value, ttl.as_secs())
            .await
            .map_err(|e| {
                cqupt_ics_core::Error::Config(format!("Failed to set Redis key: {}", e))
            })?;

        Ok(())
    }

    async fn get_raw(&self, key: &str) -> Result<Option<Vec<u8>>> {
        use redis::AsyncCommands;

        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        let result: redis::RedisResult<Option<Vec<u8>>> = conn.get(&full_key).await;
        match result {
            Ok(data) => Ok(data),
            Err(e) => Err(cqupt_ics_core::Error::Config(format!(
                "Failed to get Redis key: {}",
                e
            ))),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        use redis::AsyncCommands;

        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        conn.del::<_, ()>(&full_key).await.map_err(|e| {
            cqupt_ics_core::Error::Config(format!("Failed to delete Redis key: {}", e))
        })?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        use redis::AsyncCommands;

        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        let exists: bool = conn.exists(&full_key).await.map_err(|e| {
            cqupt_ics_core::Error::Config(format!("Failed to check Redis key existence: {}", e))
        })?;

        Ok(exists)
    }

    async fn clear(&self) -> Result<()> {
        use redis::AsyncCommands;

        let pattern = format!("{}:*", self.prefix);
        let mut conn = self.connection.clone();

        // 使用 SCAN 来获取所有匹配的键
        let keys: Vec<String> = conn.keys(&pattern).await.map_err(|e| {
            cqupt_ics_core::Error::Config(format!("Failed to scan Redis keys: {}", e))
        })?;

        if !keys.is_empty() {
            conn.del::<_, ()>(keys).await.map_err(|e| {
                cqupt_ics_core::Error::Config(format!("Failed to delete Redis keys: {}", e))
            })?;
        }

        Ok(())
    }

    async fn expire(&self, key: &str, ttl: Duration) -> Result<()> {
        use redis::AsyncCommands;

        let full_key = self.build_key(key);
        let mut conn = self.connection.clone();

        conn.expire::<_, ()>(&full_key, ttl.as_secs() as i64)
            .await
            .map_err(|e| {
                cqupt_ics_core::Error::Config(format!("Failed to set Redis key expiration: {}", e))
            })?;

        Ok(())
    }
}
