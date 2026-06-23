use anyhow::Context;
use opencord_server::config::{AppConfig, RuntimeConfig};
use opencord_server::observability::init_tracing;
use opencord_server::routes::api_router_with_state;
use opencord_server::state::AppState;
use sea_orm::Database;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime_config = RuntimeConfig::try_from_env().context("load API config")?;
    init_tracing(&runtime_config);
    let bind_addr = runtime_config.bind.api.clone();
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind API listener at {bind_addr}"))?;

    let state = app_state(&runtime_config).await?;

    tracing::info!("starting opencord-api on {bind_addr}");
    axum::serve(listener, api_router_with_state(state))
        .await
        .context("serve API")?;

    Ok(())
}

async fn app_state(runtime_config: &RuntimeConfig) -> anyhow::Result<AppState> {
    let config = AppConfig::from_runtime(runtime_config);
    let Some(database_url) = runtime_config.database.url.clone() else {
        tracing::warn!("DATABASE_URL not set; API auth store is in-memory");
        return Ok(AppState::in_memory(config));
    };

    let db = Database::connect(&database_url)
        .await
        .context("connect API database")?;

    Ok(AppState::with_database(config, db))
}
