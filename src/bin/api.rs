use anyhow::Context;
use opencord_server::config::{AppConfig, api_bind_addr};
use opencord_server::routes::api_router_with_state;
use opencord_server::state::AppState;
use sea_orm::Database;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let bind_addr = api_bind_addr();
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind API listener at {bind_addr}"))?;

    let state = app_state().await?;

    tracing::info!("starting opencord-api on {bind_addr}");
    axum::serve(listener, api_router_with_state(state))
        .await
        .context("serve API")?;

    Ok(())
}

async fn app_state() -> anyhow::Result<AppState> {
    let config = AppConfig::from_env();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        tracing::warn!("DATABASE_URL not set; API auth store is in-memory");
        return Ok(AppState::in_memory(config));
    };

    let db = Database::connect(&database_url)
        .await
        .context("connect API database")?;

    Ok(AppState::with_database(config, db))
}
