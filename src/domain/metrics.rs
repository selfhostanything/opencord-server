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

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct HttpRequestKey {
    method: String,
    status: u16,
}

#[derive(Default)]
pub struct MediaMetrics {
    http_requests_total: Mutex<BTreeMap<HttpRequestKey, u64>>,
    voice_join_success_total: AtomicU64,
    voice_join_failures_total: Mutex<BTreeMap<&'static str, u64>>,
    voice_participants: Mutex<BTreeMap<VoiceChannelKey, BTreeSet<Uuid>>>,
    kafka_produced_total: AtomicU64,
    kafka_consumed_total: AtomicU64,
    kafka_consumer_lag: Mutex<BTreeMap<String, u64>>,
    jobs_completed_total: AtomicU64,
    jobs_retried_total: AtomicU64,
    jobs_dead_lettered_total: AtomicU64,
    scylla_reads_total: AtomicU64,
    scylla_writes_total: AtomicU64,
}

impl MediaMetrics {
    pub fn record_http_request(&self, method: impl Into<String>, status: u16) {
        let mut requests = self
            .http_requests_total
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *requests
            .entry(HttpRequestKey {
                method: method.into(),
                status,
            })
            .or_insert(0) += 1;
    }

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

    pub fn record_kafka_produced(&self) {
        self.kafka_produced_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_kafka_consumed(&self) {
        self.kafka_consumed_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_kafka_consumer_lag(&self, group: impl Into<String>, lag: u64) {
        let mut lags = self
            .kafka_consumer_lag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        lags.insert(group.into(), lag);
    }

    pub fn record_job_completed(&self) {
        self.jobs_completed_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_job_retried(&self) {
        self.jobs_retried_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_job_dead_lettered(&self) {
        self.jobs_dead_lettered_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_scylla_read(&self) {
        self.scylla_reads_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_scylla_write(&self) {
        self.scylla_writes_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn render_prometheus(&self) -> String {
        let mut output = String::new();
        output.push_str("# HELP opencord_http_requests_total HTTP requests served.\n");
        output.push_str("# TYPE opencord_http_requests_total counter\n");
        let requests = self
            .http_requests_total
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (key, count) in requests.iter() {
            output.push_str(&format!(
                "opencord_http_requests_total{{method=\"{}\",status=\"{}\"}} {}\n",
                escape_label_value(&key.method),
                key.status,
                count
            ));
        }
        drop(requests);

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
        drop(participants);

        output.push_str("# HELP opencord_kafka_produced_total Kafka events and jobs produced.\n");
        output.push_str("# TYPE opencord_kafka_produced_total counter\n");
        output.push_str(&format!(
            "opencord_kafka_produced_total {}\n",
            self.kafka_produced_total.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP opencord_kafka_consumed_total Kafka events and jobs consumed.\n");
        output.push_str("# TYPE opencord_kafka_consumed_total counter\n");
        output.push_str(&format!(
            "opencord_kafka_consumed_total {}\n",
            self.kafka_consumed_total.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP opencord_kafka_consumer_lag Kafka consumer lag by group.\n");
        output.push_str("# TYPE opencord_kafka_consumer_lag gauge\n");
        let lags = self
            .kafka_consumer_lag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (group, lag) in lags.iter() {
            output.push_str(&format!(
                "opencord_kafka_consumer_lag{{group=\"{}\"}} {}\n",
                escape_label_value(group),
                lag
            ));
        }
        drop(lags);

        output.push_str("# HELP opencord_jobs_completed_total Worker jobs completed.\n");
        output.push_str("# TYPE opencord_jobs_completed_total counter\n");
        output.push_str(&format!(
            "opencord_jobs_completed_total {}\n",
            self.jobs_completed_total.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP opencord_jobs_retried_total Worker jobs scheduled for retry.\n");
        output.push_str("# TYPE opencord_jobs_retried_total counter\n");
        output.push_str(&format!(
            "opencord_jobs_retried_total {}\n",
            self.jobs_retried_total.load(Ordering::Relaxed)
        ));

        output.push_str(
            "# HELP opencord_jobs_dead_lettered_total Worker jobs sent to dead letter.\n",
        );
        output.push_str("# TYPE opencord_jobs_dead_lettered_total counter\n");
        output.push_str(&format!(
            "opencord_jobs_dead_lettered_total {}\n",
            self.jobs_dead_lettered_total.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP opencord_scylla_reads_total ScyllaDB read operations.\n");
        output.push_str("# TYPE opencord_scylla_reads_total counter\n");
        output.push_str(&format!(
            "opencord_scylla_reads_total {}\n",
            self.scylla_reads_total.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP opencord_scylla_writes_total ScyllaDB write operations.\n");
        output.push_str("# TYPE opencord_scylla_writes_total counter\n");
        output.push_str(&format!(
            "opencord_scylla_writes_total {}\n",
            self.scylla_writes_total.load(Ordering::Relaxed)
        ));

        output
    }
}

fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}
