use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::meeting::{
    Meeting, MeetingAttendee, MeetingBundle, MeetingError, MeetingReminder, MeetingStore,
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
