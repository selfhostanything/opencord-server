use anyhow::Context;
use opencord_server::config::{AppConfig, RuntimeConfig};
use opencord_server::local_seed::{LocalAlphaSeedOptions, seed_local_alpha};
use opencord_server::state::AppState;
use sea_orm::Database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let runtime = RuntimeConfig::from_env();
    let database_url = runtime
        .database
        .url
        .clone()
        .context("DATABASE_URL is required to seed OpenCord local alpha data")?;
    let db = Database::connect(&database_url)
        .await
        .context("connect to database")?;
    let state = AppState::with_database(AppConfig::from_runtime(&runtime), db);
    let report = seed_local_alpha(&state, LocalAlphaSeedOptions::from_env()).await?;

    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}
