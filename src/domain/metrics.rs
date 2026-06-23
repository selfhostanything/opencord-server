use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use uuid::Uuid;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct VoiceChannelKey {
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
}

#[derive(Default)]
pub struct MediaMetrics {
    voice_join_success_total: AtomicU64,
    voice_join_failures_total: Mutex<BTreeMap<&'static str, u64>>,
    voice_participants: Mutex<BTreeMap<VoiceChannelKey, BTreeSet<Uuid>>>,
}

impl MediaMetrics {
    pub fn record_voice_join_success(&self) {
        self.voice_join_success_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_voice_join_failure(&self, reason: &'static str) {
        let mut failures = self
            .voice_join_failures_total
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *failures.entry(reason).or_insert(0) += 1;
    }

    pub fn record_voice_participant_joined(
        &self,
        organization_id: Uuid,
        space_id: Uuid,
        channel_id: Uuid,
        user_id: Uuid,
    ) {
        let mut participants = self
            .voice_participants
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        participants
            .entry(VoiceChannelKey {
                organization_id,
                space_id,
                channel_id,
            })
            .or_default()
            .insert(user_id);
    }

    pub fn render_prometheus(&self) -> String {
        let mut output = String::new();
        output.push_str(
            "# HELP opencord_media_voice_join_success_total Successful voice channel joins.\n",
        );
        output.push_str("# TYPE opencord_media_voice_join_success_total counter\n");
        output.push_str(&format!(
            "opencord_media_voice_join_success_total {}\n",
            self.voice_join_success_total.load(Ordering::Relaxed)
        ));

        output.push_str(
            "# HELP opencord_media_voice_join_failures_total Failed voice channel joins.\n",
        );
        output.push_str("# TYPE opencord_media_voice_join_failures_total counter\n");
        let failures = self
            .voice_join_failures_total
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (reason, count) in failures.iter() {
            output.push_str(&format!(
                "opencord_media_voice_join_failures_total{{reason=\"{}\"}} {}\n",
                escape_label_value(reason),
                count
            ));
        }
        drop(failures);

        output.push_str(
            "# HELP opencord_media_voice_participants_current Process-known voice participants by channel.\n",
        );
        output.push_str("# TYPE opencord_media_voice_participants_current gauge\n");
        let participants = self
            .voice_participants
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (key, user_ids) in participants.iter() {
            output.push_str(&format!(
                "opencord_media_voice_participants_current{{organization_id=\"{}\",space_id=\"{}\",channel_id=\"{}\"}} {}\n",
                key.organization_id,
                key.space_id,
                key.channel_id,
                user_ids.len()
            ));
        }

        output
    }
}

fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}
