use std::env;

use anyhow::Context;
use opencord_server::db::migrations::Migrator;
use sea_orm::Database;
use sea_orm_migration::prelude::MigratorTrait;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url =
        env::var("DATABASE_URL").context("DATABASE_URL is required to run OpenCord migrations")?;
    let db = Database::connect(&database_url)
        .await
        .context("connect to database")?;

    Migrator::up(&db, None).await.context("run migrations")?;
    tracing::info!("database migrations complete");

    Ok(())
}
