use chrono::{Duration, Utc};
use opencord_server::scylla::{
    PresenceHeartbeat, PresenceStatus, PresenceStore, ScyllaPresenceStore,
};
use uuid::Uuid;

#[tokio::test]
async fn scylla_presence_store_writes_reads_and_filters_expired_presence() {
    if std::env::var("OPENCORD_SCYLLA_SMOKE").ok().as_deref() != Some("1") {
        eprintln!("set OPENCORD_SCYLLA_SMOKE=1 to run the local ScyllaDB smoke test");
        return;
    }

    let contact_points = std::env::var("SCYLLA_CONTACT_POINTS")
        .unwrap_or_else(|_| "localhost:9042".to_owned())
        .split(',')
        .map(str::trim)
        .filter(|contact_point| !contact_point.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let store = ScyllaPresenceStore::connect(&contact_points, "opencord_smoke")
        .await
        .expect("connect scylla");
    store.bootstrap_schema().await.expect("bootstrap schema");

    let organization_id = Uuid::now_v7();
    let space_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let now = Utc::now();

    store
        .upsert(PresenceHeartbeat {
            organization_id,
            space_id,
            user_id,
            status: PresenceStatus::Online,
            observed_at: now,
            expires_at: now + Duration::seconds(60),
        })
        .await
        .expect("upsert heartbeat");

    let active = store
        .list_active_by_space(organization_id, space_id, now + Duration::seconds(5))
        .await
        .expect("list active heartbeats");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].user_id, user_id);

    let expired = store
        .list_active_by_space(organization_id, space_id, now + Duration::seconds(90))
        .await
        .expect("list expired heartbeats");
    assert!(expired.is_empty());
}
