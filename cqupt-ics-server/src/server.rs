use std::{env, net::SocketAddr};

use anyhow::Result;
use tokio::net::TcpListener;

use crate::handlers::create_app;

pub async fn start_server(redis_url: String) -> Result<()> {
    let app = create_app(&redis_url)
        .await
        .map_err(|e| anyhow::anyhow!("初始化应用失败: {}", e))?;

    // 从环境变量获取端口，默认为3000
    let port = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tracing::info!("CQUPT ICS Server starting on {}", addr);

    let listener = TcpListener::bind(addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}
