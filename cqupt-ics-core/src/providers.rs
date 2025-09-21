//! 数据提供者模块
//!
//! 此模块定义了课程数据提供者的trait和实现。

pub mod base;
pub mod redrock;

use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    CourseRequest, CourseResponse, Result,
    cache::{CacheBackend, CacheManager},
};

pub use base::*;

/// Provider token trait for serialization
pub trait ProviderToken: Send + Sync + Serialize + DeserializeOwned {}
impl<T> ProviderToken for T where T: Send + Sync + Serialize + DeserializeOwned {}

/// 数据提供者trait
#[async_trait]
pub trait Provider: Send + Sync {
    /// Token type for this provider
    type Token: Send + Sync + Serialize + DeserializeOwned;

    /// Provider name
    fn name(&self) -> &str;

    /// Provider description
    fn description(&self) -> &str;

    /// Authenticate and get token
    async fn authenticate(&self, request: &CourseRequest) -> Result<Self::Token>;

    /// Validate existing token
    async fn validate_token(&self, token: &Self::Token) -> Result<bool>;

    /// Refresh token
    async fn refresh_token(&self, token: &Self::Token) -> Result<Self::Token>;

    /// Get courses using token
    async fn get_courses(
        &self,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<CourseResponse>;

    /// Logout and invalidate token
    async fn logout(&self, token: &Self::Token) -> Result<()>;

    /// Token TTL
    fn token_ttl(&self) -> Duration {
        Duration::from_secs(3600 * 24) // 24 hours default
    }
}

/// Provider wrapper with caching
#[async_trait]
pub trait ProviderWrapper: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Provider description
    fn description(&self) -> &str;

    /// Validate credentials
    async fn validate(&self, request: &CourseRequest) -> Result<()>;

    /// Get courses with caching
    async fn get_courses(&self, request: &mut CourseRequest) -> Result<CourseResponse>;

    /// Logout
    async fn logout(&self) -> Result<()>;
}

pub trait IntoStatic: Sized {
    fn into_static(self) -> &'static Self {
        let p: &'static mut Self = Box::leak(Box::new(self));
        &*p
    }
}

impl<T: 'static> IntoStatic for T {}

/// Wrapper implementation with caching
#[derive(Clone)]
pub struct Wrapper<P: Provider + 'static, C: CacheBackend + 'static> {
    provider: P,
    cache_manager: CacheManager<C>,
}

impl<P: Provider + 'static, C: CacheBackend + 'static> Wrapper<P, C> {
    /// Create new wrapper
    pub fn new(provider: P, cache_manager: CacheManager<C>) -> Self {
        Self {
            provider,
            cache_manager,
        }
    }

    /// Generate cache key for token
    fn token_cache_key(&self, request: &CourseRequest) -> String {
        format!(
            "{}:token:{}",
            self.provider.name(),
            request.credentials.username
        )
    }

    /// Get cached token or authenticate
    async fn get_or_create_token(&self, request: &CourseRequest) -> Result<P::Token> {
        let cache_key = self.token_cache_key(request);

        // Try to get cached token
        if let Some(token) = self
            .cache_manager
            .get_cached_token::<P::Token>(&cache_key)
            .await?
        {
            // Validate cached token
            if self.provider.validate_token(&token).await.unwrap_or(false) {
                return Ok(token);
            }

            // Try to refresh if validation failed
            if let Ok(refreshed_token) = self.provider.refresh_token(&token).await {
                let ttl = self.provider.token_ttl();
                self.cache_manager
                    .cache_token(&cache_key, &refreshed_token, ttl)
                    .await?;
                return Ok(refreshed_token);
            }

            // Remove invalid token from cache
            self.cache_manager.remove_token_cache(&cache_key).await?;
        }

        // Authenticate and cache new token
        let token = self.provider.authenticate(request).await?;
        let ttl = self.provider.token_ttl();
        self.cache_manager
            .cache_token(&cache_key, &token, ttl)
            .await?;

        Ok(token)
    }
}

#[async_trait]
impl<P: Provider + 'static, C: CacheBackend + 'static> ProviderWrapper for Wrapper<P, C> {
    fn name(&self) -> &str {
        self.provider.name()
    }

    fn description(&self) -> &str {
        self.provider.description()
    }

    async fn validate(&self, request: &CourseRequest) -> Result<()> {
        let _token = self.get_or_create_token(request).await?;
        Ok(())
    }

    async fn get_courses(&self, request: &mut CourseRequest) -> Result<CourseResponse> {
        let token = self.get_or_create_token(request).await?;
        self.provider.get_courses(request, &token).await
    }

    async fn logout(&self) -> Result<()> {
        // Clear all cached tokens for this provider
        self.cache_manager.clear().await?;
        Ok(())
    }
}

/// Provider registry
pub struct ProviderRegistry {
    providers: HashMap<String, &'static dyn ProviderWrapper>,
}

impl ProviderRegistry {
    /// Create new registry
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider
    pub fn register(&mut self, provider: &'static dyn ProviderWrapper) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// Get provider by name
    pub fn get_provider(&self, name: &str) -> Option<&'static dyn ProviderWrapper> {
        self.providers.get(name).copied()
    }

    /// List all providers
    pub fn list_providers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.providers.values().map(|p| (p.name(), p.description()))
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
