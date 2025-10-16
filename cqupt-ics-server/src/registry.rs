use crate::cache::RedisCache;
use cqupt_ics_core::prelude::{redrock::RedrockProvider, wecqupt::WecquptProvider, *};

pub(crate) async fn init_with_redis(
    redis_manager: &redis::aio::ConnectionManager,
) -> Result<ProviderRegistry, cqupt_ics_core::Error> {
    let mut p = ProviderRegistry::new();

    let redis_cache = RedisCache::new("cqupt-ics".to_string(), redis_manager.clone());

    p.register(
        Wrapper::new(
            RedrockProvider::new(),
            CacheManager::new(redis_cache.clone()),
        )
        .into_static(),
    );
    p.register(
        Wrapper::new(
            WecquptProvider::new(),
            CacheManager::new(redis_cache.clone()),
        )
        .into_static(),
    );

    Ok(p)
}
