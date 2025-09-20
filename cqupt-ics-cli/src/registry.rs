use std::sync::OnceLock;

use cqupt_ics_core::{
    cache::CacheManager,
    prelude::ProviderRegistry,
    providers::{Wrapper, redrock::RedrockProvider},
};

use crate::cache::FileCache;

pub static REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();

pub(crate) fn init() {
    let mut p = ProviderRegistry::new();
    p.register(Wrapper::static_ref(
        RedrockProvider::new(),
        CacheManager::new(FileCache::with_default_dir("cqupt-ics").unwrap()),
    ));
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
