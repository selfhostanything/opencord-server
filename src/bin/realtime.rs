use anyhow::Context;
use opencord_server::config::{AppConfig, realtime_bind_addr};
use opencord_server::routes::realtime_router_with_state;
use opencord_server::state::AppState;
use sea_orm::Database;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let bind_addr = realtime_bind_addr();
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind realtime listener at {bind_addr}"))?;

    let state = app_state().await?;

    tracing::info!("starting opencord-realtime on {bind_addr}");
    axum::serve(listener, realtime_router_with_state(state))
        .await
        .context("serve realtime gateway")?;

    Ok(())
}

async fn app_state() -> anyhow::Result<AppState> {
    let config = AppConfig::from_env();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        tracing::warn!("DATABASE_URL not set; realtime gateway auth store is in-memory");
        return Ok(AppState::in_memory(config));
    };

    let db = Database::connect(&database_url)
        .await
        .context("connect realtime database")?;

    Ok(AppState::with_database(config, db))
}
