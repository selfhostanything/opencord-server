use anyhow::Context;
use opencord_server::config::{AppConfig, realtime_bind_addr};
use opencord_server::routes::health_router;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let bind_addr = realtime_bind_addr();
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind realtime listener at {bind_addr}"))?;

    tracing::info!("starting opencord-realtime on {bind_addr}");
    axum::serve(listener, health_router(AppConfig::from_env()))
        .await
        .context("serve realtime health")?;

    Ok(())
}
