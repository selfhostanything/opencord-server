use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::meeting::{
    Meeting, MeetingAttendee, MeetingBundle, MeetingError, MeetingReminder, MeetingReminderJob,
    MeetingStore,
};

#[derive(Default)]
pub struct MemoryMeetingStore {
    state: Mutex<MemoryMeetingState>,
}

#[derive(Default)]
struct MemoryMeetingState {
    meetings_by_id: HashMap<Uuid, Meeting>,
    attendees_by_meeting_id: HashMap<Uuid, Vec<MeetingAttendee>>,
    reminders_by_meeting_id: HashMap<Uuid, Vec<MeetingReminder>>,
}

#[async_trait::async_trait]
impl MeetingStore for MemoryMeetingStore {
    async fn create_meeting(&self, bundle: MeetingBundle) -> Result<(), MeetingError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| MeetingError::StoreUnavailable)?;
        state
            .attendees_by_meeting_id
            .insert(bundle.meeting.id, bundle.attendees);
        state
            .reminders_by_meeting_id
            .insert(bundle.meeting.id, bundle.reminders);
        state
            .meetings_by_id
            .insert(bundle.meeting.id, bundle.meeting);

        Ok(())
    }

    async fn list_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<MeetingBundle>, MeetingError> {
        let state = self
            .state
            .lock()
            .map_err(|_| MeetingError::StoreUnavailable)?;
        let mut meetings = state
            .meetings_by_id
            .values()
            .filter(|meeting| meeting.organization_id == organization_id)
            .cloned()
            .map(|meeting| bundle_for_meeting(&state, meeting))
            .collect::<Vec<_>>();

        meetings.sort_by(|left, right| {
            left.meeting
                .starts_at
                .cmp(&right.meeting.starts_at)
                .then_with(|| left.meeting.id.cmp(&right.meeting.id))
        });
        Ok(meetings)
    }

    async fn get_meeting(&self, meeting_id: Uuid) -> Result<Option<MeetingBundle>, MeetingError> {
        let state = self
            .state
            .lock()
            .map_err(|_| MeetingError::StoreUnavailable)?;
        Ok(state
            .meetings_by_id
            .get(&meeting_id)
            .cloned()
            .map(|meeting| bundle_for_meeting(&state, meeting)))
    }

    async fn get_meeting_by_join_slug(
        &self,
        join_slug: String,
    ) -> Result<Option<MeetingBundle>, MeetingError> {
        let state = self
            .state
            .lock()
            .map_err(|_| MeetingError::StoreUnavailable)?;
        Ok(state
            .meetings_by_id
            .values()
            .find(|meeting| meeting.join_slug == join_slug)
            .cloned()
            .map(|meeting| bundle_for_meeting(&state, meeting)))
    }

    async fn update_meeting(&self, meeting: Meeting) -> Result<MeetingBundle, MeetingError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| MeetingError::StoreUnavailable)?;
        if !state.meetings_by_id.contains_key(&meeting.id) {
            return Err(MeetingError::NotFound);
        }
        state.meetings_by_id.insert(meeting.id, meeting.clone());

        Ok(bundle_for_meeting(&state, meeting))
    }

    async fn list_due_reminders(
        &self,
        due_at: String,
        limit: usize,
    ) -> Result<Vec<MeetingReminderJob>, MeetingError> {
        if limit == 0 {
            return Ok(vec![]);
        }

        let state = self
            .state
            .lock()
            .map_err(|_| MeetingError::StoreUnavailable)?;
        let mut jobs = state
            .reminders_by_meeting_id
            .values()
            .flatten()
            .filter(|reminder| reminder.status == "pending" && reminder.scheduled_for <= due_at)
            .filter_map(|reminder| {
                let meeting = state.meetings_by_id.get(&reminder.meeting_id)?;
                (meeting.status == "scheduled").then(|| MeetingReminderJob {
                    meeting: meeting.clone(),
                    reminder: reminder.clone(),
                })
            })
            .collect::<Vec<_>>();

        jobs.sort_by(|left, right| {
            left.reminder
                .scheduled_for
                .cmp(&right.reminder.scheduled_for)
                .then_with(|| left.reminder.id.cmp(&right.reminder.id))
        });
        jobs.truncate(limit);

        Ok(jobs)
    }

    async fn mark_reminder_sent(
        &self,
        reminder_id: Uuid,
        _sent_at: String,
    ) -> Result<(), MeetingError> {
        update_reminder_status(&self.state, reminder_id, "sent")
    }

    async fn mark_reminder_failed(
        &self,
        reminder_id: Uuid,
        _failed_at: String,
        _failure_reason: String,
    ) -> Result<(), MeetingError> {
        update_reminder_status(&self.state, reminder_id, "failed")
    }
}

fn bundle_for_meeting(state: &MemoryMeetingState, meeting: Meeting) -> MeetingBundle {
    MeetingBundle {
        attendees: state
            .attendees_by_meeting_id
            .get(&meeting.id)
            .cloned()
            .unwrap_or_default(),
        reminders: state
            .reminders_by_meeting_id
            .get(&meeting.id)
            .cloned()
            .unwrap_or_default(),
        meeting,
    }
}

fn update_reminder_status(
    state: &Mutex<MemoryMeetingState>,
    reminder_id: Uuid,
    status: &str,
) -> Result<(), MeetingError> {
    let mut state = state.lock().map_err(|_| MeetingError::StoreUnavailable)?;
    for reminders in state.reminders_by_meeting_id.values_mut() {
        if let Some(reminder) = reminders
            .iter_mut()
            .find(|candidate| candidate.id == reminder_id && candidate.status == "pending")
        {
            reminder.status = status.to_owned();
            return Ok(());
        }
    }

    Err(MeetingError::NotFound)
}
