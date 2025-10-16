mod cache;
mod handlers;
mod registry;
mod server;

use anyhow::Result;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use std::env;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cqupt_ics_server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 获取Redis URL
    let redis_url = env::var("REDIS_URL")
        .map_err(|_| anyhow::anyhow!("REDIS_URL environment variable is required"))?;
    let manager = ConnectionManager::new_with_config(
        redis::Client::open(redis_url).expect("Invalid Redis URL"),
        ConnectionManagerConfig::default(),
    )
    .await
    .expect("Init Redis Connection Manager failed");

    // 初始化Provider注册表
    let r = registry::init_with_redis(&manager)
        .await
        .inspect_err(|e| tracing::error!("Failed to initialize provider registry: {}", e))?;

    // 启动服务器
    server::start_server(&manager, r).await
}
