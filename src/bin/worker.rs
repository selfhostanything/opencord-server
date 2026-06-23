use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use opencord_server::config::{AppConfig, worker_bind_addr};
use opencord_server::domain::meeting::MeetingStore;
use opencord_server::domain::reminder::{LoggingMeetingReminderDispatcher, MeetingReminderWorker};
use opencord_server::repositories::meeting_memory::MemoryMeetingStore;
use opencord_server::repositories::meeting_postgres::PostgresMeetingStore;
use opencord_server::routes::health_router;
use sea_orm::Database;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = AppConfig::from_env();
    let reminder_store = reminder_store().await?;
    let reminder_worker =
        MeetingReminderWorker::new(reminder_store, Arc::new(LoggingMeetingReminderDispatcher))
            .with_batch_size(reminder_batch_size());
    tokio::spawn(run_reminder_loop(reminder_worker, reminder_interval()));

    let bind_addr = worker_bind_addr();
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("bind worker listener at {bind_addr}"))?;

    tracing::info!("starting opencord-worker on {bind_addr}");
    axum::serve(listener, health_router(config))
        .await
        .context("serve worker health")?;

    Ok(())
}

async fn reminder_store() -> anyhow::Result<Arc<dyn MeetingStore>> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        tracing::warn!("DATABASE_URL not set; reminder worker is using an in-memory store");
        return Ok(Arc::new(MemoryMeetingStore::default()));
    };

    let db = Database::connect(&database_url)
        .await
        .context("connect worker database")?;

    Ok(Arc::new(PostgresMeetingStore::new(db)))
}

async fn run_reminder_loop(worker: MeetingReminderWorker, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);

    loop {
        ticker.tick().await;
        match worker.run_once().await {
            Ok(summary) if summary.scanned > 0 => {
                tracing::info!(
                    scanned = summary.scanned,
                    sent = summary.sent,
                    failed = summary.failed,
                    "processed meeting reminders"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::error!(
                    code = error.code(),
                    message = error.message(),
                    "meeting reminder worker failed"
                );
            }
        }
    }
}

fn reminder_interval() -> Duration {
    Duration::from_secs(
        std::env::var("OPENCORD_REMINDER_POLL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .unwrap_or(30),
    )
}

fn reminder_batch_size() -> usize {
    std::env::var("OPENCORD_REMINDER_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|size| *size > 0)
        .unwrap_or(100)
}
