use anyhow::Context;
use opencord_server::config::{AppConfig, worker_bind_addr};
use opencord_server::routes::health_router;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let bind_addr = worker_bind_addr();
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind worker listener at {bind_addr}"))?;

    tracing::info!("starting opencord-worker on {bind_addr}");
    axum::serve(listener, health_router(AppConfig::from_env()))
        .await
        .context("serve worker health")?;

    Ok(())
}
