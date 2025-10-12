mod cache;
mod handlers;
mod registry;
mod server;

use std::env;

use anyhow::Result;
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

    // 初始化Provider注册表
    if let Err(e) = registry::init_with_redis(&redis_url).await {
        tracing::error!("Failed to initialize provider registry: {}", e);
        return Err(e.into());
    }

    // 启动服务器
    server::start_server(redis_url).await
}
