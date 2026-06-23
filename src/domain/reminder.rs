use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

use crate::domain::meeting::{MeetingError, MeetingReminderJob, MeetingStore};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingReminderDelivery {
    pub reminder_id: Uuid,
    pub meeting_id: Uuid,
    pub meeting_title: String,
    pub meeting_starts_at: String,
    pub join_slug: String,
    pub channel: String,
    pub recipient_user_id: Option<Uuid>,
    pub recipient_email: Option<String>,
    pub scheduled_for: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReminderRunSummary {
    pub scanned: usize,
    pub sent: usize,
    pub failed: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReminderDeliveryError {
    message: String,
}

impl ReminderDeliveryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[async_trait::async_trait]
pub trait MeetingReminderDispatcher: Send + Sync {
    async fn deliver(&self, delivery: MeetingReminderDelivery)
    -> Result<(), ReminderDeliveryError>;
}

#[derive(Clone)]
pub struct MeetingReminderWorker {
    store: Arc<dyn MeetingStore>,
    dispatcher: Arc<dyn MeetingReminderDispatcher>,
    batch_size: usize,
}

impl MeetingReminderWorker {
    pub fn new(
        store: Arc<dyn MeetingStore>,
        dispatcher: Arc<dyn MeetingReminderDispatcher>,
    ) -> Self {
        Self {
            store,
            dispatcher,
            batch_size: 100,
        }
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size.max(1);
        self
    }

    pub async fn run_once(&self) -> Result<ReminderRunSummary, MeetingError> {
        self.run_once_at(Utc::now()).await
    }

    pub async fn run_once_at(
        &self,
        now: DateTime<Utc>,
    ) -> Result<ReminderRunSummary, MeetingError> {
        let now = format_time(now);
        let jobs = self
            .store
            .list_due_reminders(now.clone(), self.batch_size)
            .await?;
        let mut summary = ReminderRunSummary {
            scanned: jobs.len(),
            sent: 0,
            failed: 0,
        };

        for job in jobs {
            let reminder_id = job.reminder.id;
            let delivery = delivery_from_job(job);
            match self.dispatcher.deliver(delivery).await {
                Ok(()) => {
                    self.store
                        .mark_reminder_sent(reminder_id, now.clone())
                        .await?;
                    summary.sent += 1;
                }
                Err(error) => {
                    self.store
                        .mark_reminder_failed(reminder_id, now.clone(), error.message().to_owned())
                        .await?;
                    summary.failed += 1;
                }
            }
        }

        Ok(summary)
    }
}

#[derive(Default)]
pub struct LoggingMeetingReminderDispatcher;

#[async_trait::async_trait]
impl MeetingReminderDispatcher for LoggingMeetingReminderDispatcher {
    async fn deliver(
        &self,
        delivery: MeetingReminderDelivery,
    ) -> Result<(), ReminderDeliveryError> {
        tracing::info!(
            channel = %delivery.channel,
            reminder_id = %delivery.reminder_id,
            meeting_id = %delivery.meeting_id,
            recipient_user_id = ?delivery.recipient_user_id,
            recipient_email = ?delivery.recipient_email,
            "meeting reminder fired"
        );
        Ok(())
    }
}

fn delivery_from_job(job: MeetingReminderJob) -> MeetingReminderDelivery {
    MeetingReminderDelivery {
        reminder_id: job.reminder.id,
        meeting_id: job.meeting.id,
        meeting_title: job.meeting.title,
        meeting_starts_at: job.meeting.starts_at,
        join_slug: job.meeting.join_slug,
        channel: job.reminder.channel,
        recipient_user_id: job.reminder.recipient_user_id,
        recipient_email: job.reminder.recipient_email,
        scheduled_for: job.reminder.scheduled_for,
    }
}

fn format_time(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Secs, true)
}
