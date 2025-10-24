pub mod base;
pub mod redrock;
pub mod wecqupt;

use std::{collections::HashMap, time::Duration};

use async_trait::async_trait;
use chrono::FixedOffset;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    CourseRequest, CourseResponse, Result,
    cache::{CacheBackend, CacheManager},
};

pub use base::*;

/// Provider token trait for serialization
pub trait ProviderToken: Send + Sync + Serialize + DeserializeOwned {}
impl<T> ProviderToken for T where T: Send + Sync + Serialize + DeserializeOwned {}
// Context 变成一个简单的 newtype wrapper
#[derive(Debug)]
pub struct Context<T> {
    inner: Option<T>,
}

impl<T> Context<T> {
    pub fn new(value: T) -> Self {
        Self { inner: Some(value) }
    }

    pub fn set(&mut self, value: T) {
        self.inner = Some(value);
    }

    pub fn get_mut(&mut self) -> &mut Option<T> {
        &mut self.inner
    }

    pub fn get(&self) -> &Option<T> {
        &self.inner
    }

    pub fn as_ref(&self) -> Option<&T> {
        self.inner.as_ref()
    }

    pub fn as_mut(&mut self) -> Option<&mut T> {
        self.inner.as_mut()
    }

    pub fn as_param(&mut self) -> ParamContext<'_, T> {
        Some(self)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_none()
    }

    pub fn is_some(&self) -> bool {
        self.inner.is_some()
    }

    pub fn ensure_valid(&mut self) -> Result<&mut Self> {
        if self.is_empty() {
            Err(crate::Error::Config("Context is empty".to_string()))
        } else {
            Ok(self)
        }
    }

    pub fn with<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(ParamContext<'_, T>) -> R,
    {
        f(Some(self))
    }
}

impl<T> Default for Context<T> {
    fn default() -> Self {
        Self { inner: None }
    }
}

impl<T: Clone> Clone for Context<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

type ParamContext<'a, T> = Option<&'a mut Context<T>>;

/// ParamContext的便利扩展
pub trait ParamContextExt<'a, T> {
    /// 确保ParamContext有效并返回可变引用
    fn ensure_valid(self) -> Result<&'a mut Context<T>>;

    /// 安全地使用ParamContext执行操作
    fn use_context<F, R>(self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Context<T>) -> Result<R>;
}

impl<'a, T> ParamContextExt<'a, T> for ParamContext<'a, T> {
    fn ensure_valid(self) -> Result<&'a mut Context<T>> {
        self.ok_or_else(|| crate::Error::Config("Invalid context".to_string()))
    }

    fn use_context<F, R>(self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Context<T>) -> Result<R>,
    {
        let ctx = self.ensure_valid()?;
        f(ctx)
    }
}

/// 数据提供者trait
#[async_trait]
pub trait Provider: Send + Sync {
    /// Token type for this provider
    type Token: Send + Sync + Serialize + DeserializeOwned;
    type ContextType: Send + Sync;
    /// Provider name
    fn name(&self) -> &str;

    /// Provider description
    fn description(&self) -> &str;

    /// Get timezone for this provider
    ///
    /// Returns the timezone used by this provider for time calculations.
    /// This is used to ensure consistent timezone handling across all
    /// provider operations.
    fn timezone(&self) -> FixedOffset;

    /// Authenticate and get token
    async fn authenticate<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &CourseRequest,
    ) -> Result<Self::Token>;

    /// Validate existing token
    async fn validate_token(&self, token: &Self::Token) -> Result<bool>;

    /// Refresh token
    async fn refresh_token(&self, token: &Self::Token) -> Result<Self::Token>;

    /// Get courses using token
    /// request.semester should be Some before calling this method
    /// If use crate::providers::Wrapper, it will ensure semester is Some
    async fn get_courses<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<CourseResponse>;

    /// Get semester start date
    /// This is called if request.semester is None before get_courses
    /// If you use crate::providers::Wrapper, it will call this method automatically if request.semester is None
    /// You can use the context to store intermediate data if needed
    async fn get_semester_start<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<chrono::DateTime<FixedOffset>>;

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
    async fn logout(&self, request: &CourseRequest) -> Result<()>;
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
        let token = self.provider.authenticate(None, request).await?;
        let ttl = self.provider.token_ttl();
        self.cache_manager
            .cache_token(&cache_key, &token, ttl)
            .await?;

        Ok(token)
    }
    async fn get_courses_once(&self, request: &mut CourseRequest) -> Result<CourseResponse> {
        let token = self.get_or_create_token(request).await?;
        let mut c: Context<P::ContextType> = Context::default();
        if request.semester.is_none() {
            let sem = self
                .provider
                .get_semester_start(c.as_param(), request, &token)
                .await?;
            request.semester = Some(crate::Semester { start_date: sem });
        }
        self.provider
            .get_courses(c.as_param(), request, &token)
            .await
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
        match self.get_courses_once(request).await {
            Ok(courses) => Ok(courses),
            Err(e) => {
                // On Auth error, clear the token cache and retry once
                if matches!(
                    e,
                    crate::Error::Authentication(_) | crate::Error::Provider { .. }
                ) {
                    self.logout(request).await?;
                }
                self.get_courses_once(request).await
            }
        }
    }

    async fn logout(&self, request: &CourseRequest) -> Result<()> {
        self.cache_manager
            .remove_token_cache(&self.token_cache_key(request))
            .await?;
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
