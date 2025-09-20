use std::sync::OnceLock;

use cqupt_ics_core::{
    cache::CacheManager,
    prelude::ProviderRegistry,
    providers::{Wrapper, redrock::RedrockProvider},
};

use crate::cache::RedisCache;

pub static REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();

pub(crate) async fn init_with_redis(redis_url: &str) -> Result<(), cqupt_ics_core::Error> {
    let mut p = ProviderRegistry::new();

    tracing::info!(
        "Initializing provider registry with Redis cache: {}",
        redis_url
    );
    let redis_cache = RedisCache::new(redis_url, Some("cqupt-ics".to_string())).await?;
    p.register(Wrapper::static_ref(
        RedrockProvider::new(),
        CacheManager::new(redis_cache),
    ));

    REGISTRY.set(p).map_err(|_| {
        cqupt_ics_core::Error::Config("Failed to initialize provider registry".to_string())
    })?;

    Ok(())
}

pub(crate) fn get_provider(
    name: &str,
) -> Option<&'static dyn cqupt_ics_core::providers::ProviderWrapper> {
    REGISTRY.get()?.get_provider(name)
}

pub(crate) fn list_providers() -> Box<dyn Iterator<Item = (&'static str, &'static str)>> {
    match REGISTRY.get() {
        Some(registry) => Box::new(registry.list_providers()),
        None => Box::new(std::iter::empty()),
    }
}
