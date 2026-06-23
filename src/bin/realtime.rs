use anyhow::Context;
use opencord_server::config::{AppConfig, RuntimeConfig};
use opencord_server::observability::init_tracing;
use opencord_server::routes::realtime_router_with_state;
use opencord_server::state::AppState;
use sea_orm::Database;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime_config = RuntimeConfig::try_from_env().context("load realtime config")?;
    init_tracing(&runtime_config);
    let bind_addr = runtime_config.bind.realtime.clone();
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind realtime listener at {bind_addr}"))?;

    let state = app_state(&runtime_config).await?;

    tracing::info!("starting opencord-realtime on {bind_addr}");
    axum::serve(listener, realtime_router_with_state(state))
        .await
        .context("serve realtime gateway")?;

    Ok(())
}

async fn app_state(runtime_config: &RuntimeConfig) -> anyhow::Result<AppState> {
    let config = AppConfig::from_runtime(runtime_config);
    let Some(database_url) = runtime_config.database.url.clone() else {
        tracing::warn!("DATABASE_URL not set; realtime gateway auth store is in-memory");
        return Ok(AppState::in_memory(config));
    };

    let db = Database::connect(&database_url)
        .await
        .context("connect realtime database")?;

    Ok(AppState::with_database(config, db))
}
