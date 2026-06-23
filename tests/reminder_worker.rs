use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use opencord_server::domain::ids;
use opencord_server::domain::meeting::{MeetingService, NewMeetingReminder};
use opencord_server::domain::reminder::{
    MeetingReminderDelivery, MeetingReminderDispatcher, MeetingReminderWorker,
    ReminderDeliveryError,
};
use opencord_server::repositories::meeting_memory::MemoryMeetingStore;
use uuid::Uuid;

#[derive(Clone, Default)]
struct RecordingDispatcher {
    deliveries: Arc<Mutex<Vec<MeetingReminderDelivery>>>,
}

#[async_trait::async_trait]
impl MeetingReminderDispatcher for RecordingDispatcher {
    async fn deliver(
        &self,
        delivery: MeetingReminderDelivery,
    ) -> Result<(), ReminderDeliveryError> {
        self.deliveries
            .lock()
            .expect("delivery recorder lock should not be poisoned")
            .push(delivery);
        Ok(())
    }
}

impl RecordingDispatcher {
    fn deliveries(&self) -> Vec<MeetingReminderDelivery> {
        self.deliveries
            .lock()
            .expect("delivery recorder lock should not be poisoned")
            .clone()
    }
}

#[tokio::test]
async fn worker_dispatches_due_in_app_and_email_reminders_once() {
    let store = Arc::new(MemoryMeetingStore::default());
    let meetings = MeetingService::new(store.clone());
    let recipient_user_id = ids::new_uuid_v7();

    let bundle = meetings
        .create(
            ids::new_uuid_v7(),
            None,
            None,
            ids::new_uuid_v7(),
            "Reminder Review".to_owned(),
            None,
            "2026-06-24T09:00:00Z".to_owned(),
            "2026-06-24T09:30:00Z".to_owned(),
            None,
            vec![],
            vec![
                NewMeetingReminder {
                    recipient_user_id: Some(recipient_user_id),
                    recipient_email: None,
                    channel: "in_app".to_owned(),
                    offset_minutes: 15,
                },
                NewMeetingReminder {
                    recipient_user_id: None,
                    recipient_email: Some("guest@example.com".to_owned()),
                    channel: "email".to_owned(),
                    offset_minutes: 10,
                },
                NewMeetingReminder {
                    recipient_user_id: Some(recipient_user_id),
                    recipient_email: None,
                    channel: "push".to_owned(),
                    offset_minutes: 5,
                },
            ],
        )
        .await
        .expect("meeting should be created");

    let dispatcher = RecordingDispatcher::default();
    let worker = MeetingReminderWorker::new(store, Arc::new(dispatcher.clone()));
    let now = parse_time("2026-06-24T08:50:00Z");

    let summary = worker
        .run_once_at(now)
        .await
        .expect("due reminders should dispatch");
    assert_eq!(summary.scanned, 2);
    assert_eq!(summary.sent, 2);
    assert_eq!(summary.failed, 0);

    let deliveries = dispatcher.deliveries();
    assert_eq!(deliveries.len(), 2);
    assert!(deliveries.iter().any(|delivery| {
        delivery.channel == "in_app"
            && delivery.recipient_user_id == Some(recipient_user_id)
            && delivery.meeting_title == "Reminder Review"
    }));
    assert!(deliveries.iter().any(|delivery| {
        delivery.channel == "email"
            && delivery.recipient_email.as_deref() == Some("guest@example.com")
            && delivery.join_slug == bundle.meeting.join_slug
    }));

    let updated = meetings
        .get(bundle.meeting.id)
        .await
        .expect("meeting should still exist");
    assert_eq!(
        reminder_status(&updated.reminders, "in_app", Some(recipient_user_id), None),
        Some("sent")
    );
    assert_eq!(
        reminder_status(&updated.reminders, "email", None, Some("guest@example.com")),
        Some("sent")
    );
    assert_eq!(
        reminder_status(&updated.reminders, "push", Some(recipient_user_id), None),
        Some("pending")
    );

    let second_summary = worker
        .run_once_at(now)
        .await
        .expect("sent reminders should not be redelivered");
    assert_eq!(second_summary.scanned, 0);
    assert_eq!(dispatcher.deliveries().len(), 2);
}

fn reminder_status<'a>(
    reminders: &'a [opencord_server::domain::meeting::MeetingReminder],
    channel: &str,
    recipient_user_id: Option<Uuid>,
    recipient_email: Option<&str>,
) -> Option<&'a str> {
    reminders
        .iter()
        .find(|reminder| {
            reminder.channel == channel
                && reminder.recipient_user_id == recipient_user_id
                && reminder.recipient_email.as_deref() == recipient_email
        })
        .map(|reminder| reminder.status.as_str())
}

fn parse_time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("test timestamp should parse")
        .with_timezone(&Utc)
}
