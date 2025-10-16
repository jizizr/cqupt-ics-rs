use std::sync::OnceLock;

use cqupt_ics_core::{
    cache::CacheManager,
    prelude::{redrock::RedrockProvider, wecqupt::WecquptProvider, *},
};

use crate::cache::FileCache;

pub static REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();

pub(crate) fn init() {
    let mut p = ProviderRegistry::new();
    let file_cache = FileCache::with_default_dir("cqupt-ics").unwrap();
    p.register(
        Wrapper::new(
            RedrockProvider::new(),
            CacheManager::new(file_cache.clone()),
        )
        .into_static(),
    );

    p.register(
        Wrapper::new(
            WecquptProvider::new(),
            CacheManager::new(file_cache.clone()),
        )
        .into_static(),
    );

    REGISTRY
        .set(p)
        .unwrap_or_else(|_| panic!("Failed to initialize provider registry"));
}

pub(crate) fn get_provider(
    name: &str,
) -> Option<&'static dyn cqupt_ics_core::providers::ProviderWrapper> {
    if REGISTRY.get().is_none() {
        init();
    }
    REGISTRY.get().unwrap().get_provider(name)
}

pub(crate) fn list_providers() -> impl Iterator<Item = (&'static str, &'static str)> {
    REGISTRY.get().unwrap().list_providers()
}
