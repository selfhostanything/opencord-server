use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use opencord_server::config::{AppConfig, worker_bind_addr};
use opencord_server::domain::attachment::AttachmentStore;
use opencord_server::domain::audit::AuditStore;
use opencord_server::domain::meeting::MeetingStore;
use opencord_server::domain::message::MessageStore;
use opencord_server::domain::reminder::{LoggingMeetingReminderDispatcher, MeetingReminderWorker};
use opencord_server::domain::retention::{RetentionStore, RetentionWorker};
use opencord_server::repositories::attachment_memory::MemoryAttachmentStore;
use opencord_server::repositories::attachment_postgres::PostgresAttachmentStore;
use opencord_server::repositories::audit_memory::MemoryAuditStore;
use opencord_server::repositories::audit_postgres::PostgresAuditStore;
use opencord_server::repositories::meeting_memory::MemoryMeetingStore;
use opencord_server::repositories::meeting_postgres::PostgresMeetingStore;
use opencord_server::repositories::message_memory::MemoryMessageStore;
use opencord_server::repositories::message_postgres::PostgresMessageStore;
use opencord_server::repositories::retention_memory::MemoryRetentionStore;
use opencord_server::repositories::retention_postgres::PostgresRetentionStore;
use opencord_server::routes::health_router;
use sea_orm::Database;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = AppConfig::from_env();
    let stores = worker_stores().await?;
    let reminder_worker =
        MeetingReminderWorker::new(stores.meetings, Arc::new(LoggingMeetingReminderDispatcher))
            .with_batch_size(reminder_batch_size());
    tokio::spawn(run_reminder_loop(reminder_worker, reminder_interval()));
    tokio::spawn(run_retention_loop(
        stores.retention_worker,
        retention_interval(),
        retention_dry_run(),
    ));

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

struct WorkerStores {
    meetings: Arc<dyn MeetingStore>,
    retention_worker: RetentionWorker,
}

async fn worker_stores() -> anyhow::Result<WorkerStores> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        tracing::warn!("DATABASE_URL not set; worker stores are in-memory");
        let retention: Arc<dyn RetentionStore> = Arc::new(MemoryRetentionStore::default());
        let messages: Arc<dyn MessageStore> = Arc::new(MemoryMessageStore::default());
        let attachments: Arc<dyn AttachmentStore> = Arc::new(MemoryAttachmentStore::default());
        let audit: Arc<dyn AuditStore> = Arc::new(MemoryAuditStore::default());
        return Ok(WorkerStores {
            meetings: Arc::new(MemoryMeetingStore::default()),
            retention_worker: RetentionWorker::new(retention, messages, attachments, audit),
        });
    };

    let db = Database::connect(&database_url)
        .await
        .context("connect worker database")?;

    let retention: Arc<dyn RetentionStore> = Arc::new(PostgresRetentionStore::new(db.clone()));
    let messages: Arc<dyn MessageStore> = Arc::new(PostgresMessageStore::new(db.clone()));
    let attachments: Arc<dyn AttachmentStore> = Arc::new(PostgresAttachmentStore::new(db.clone()));
    let audit: Arc<dyn AuditStore> = Arc::new(PostgresAuditStore::new(db.clone()));

    Ok(WorkerStores {
        meetings: Arc::new(PostgresMeetingStore::new(db)),
        retention_worker: RetentionWorker::new(retention, messages, attachments, audit),
    })
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

async fn run_retention_loop(worker: RetentionWorker, interval: Duration, dry_run: bool) {
    let mut ticker = tokio::time::interval(interval);

    loop {
        ticker.tick().await;
        match worker.run_once(dry_run).await {
            Ok(summary) if summary.organizations_scanned > 0 => {
                tracing::info!(
                    organizations_scanned = summary.organizations_scanned,
                    messages_purged = summary.messages_purged,
                    files_purged = summary.files_purged,
                    audit_events_purged = summary.audit_events_purged,
                    dry_run = summary.dry_run,
                    "processed retention policies"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::error!(
                    code = error.code(),
                    message = error.message(),
                    "retention worker failed"
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

fn retention_interval() -> Duration {
    Duration::from_secs(
        std::env::var("OPENCORD_RETENTION_POLL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .unwrap_or(3600),
    )
}

fn retention_dry_run() -> bool {
    std::env::var("OPENCORD_RETENTION_DRY_RUN")
        .map(|value| value != "false" && value != "0")
        .unwrap_or(true)
}
