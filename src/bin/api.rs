use anyhow::Context;
use opencord_server::config::{AppConfig, api_bind_addr};
use opencord_server::routes::api_router;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let bind_addr = api_bind_addr();
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind API listener at {bind_addr}"))?;

    tracing::info!("starting opencord-api on {bind_addr}");
    axum::serve(listener, api_router(AppConfig::from_env()))
        .await
        .context("serve API")?;

    Ok(())
}
