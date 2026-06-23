use std::fs;
use std::path::PathBuf;

fn repo_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(path).expect("repo file should be readable")
}

#[test]
fn compat_gateway_session_migration_defines_durable_sessions() {
    let migration = repo_file("src/db/migrations/m20260623072000_compat_gateway_sessions.rs");

    for expected in [
        "CREATE TABLE IF NOT EXISTS compat_gateway_sessions",
        "session_id text PRIMARY KEY",
        "application_id uuid NOT NULL REFERENCES bot_applications(id) ON DELETE CASCADE",
        "organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE",
        "bot_user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE",
        "sequence bigint NOT NULL DEFAULT 0",
        "intents bigint NOT NULL DEFAULT 0",
        "expires_at timestamptz NOT NULL DEFAULT (now() + interval '24 hours')",
        "CONSTRAINT compat_gateway_sessions_sequence_check",
        "CONSTRAINT compat_gateway_sessions_intents_check",
        "CREATE INDEX IF NOT EXISTS idx_compat_gateway_sessions_application",
        "CREATE INDEX IF NOT EXISTS idx_compat_gateway_sessions_expires_at",
    ] {
        assert!(
            migration.contains(expected),
            "compat gateway migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623072000_compat_gateway_sessions;"));
    assert!(migrator.contains("Box::new(m20260623072000_compat_gateway_sessions::Migration)"));
}

#[test]
fn compat_gateway_replay_event_migration_defines_durable_replay_journal() {
    let migration = repo_file("src/db/migrations/m20260623073000_compat_gateway_replay_events.rs");

    for expected in [
        "CREATE TABLE IF NOT EXISTS compat_gateway_replay_events",
        "session_id text NOT NULL REFERENCES compat_gateway_sessions(session_id) ON DELETE CASCADE",
        "sequence bigint NOT NULL",
        "event_type text NOT NULL",
        "payload jsonb NOT NULL",
        "CONSTRAINT compat_gateway_replay_events_sequence_check",
        "PRIMARY KEY (session_id, sequence)",
        "CREATE INDEX IF NOT EXISTS idx_compat_gateway_replay_events_created_at",
    ] {
        assert!(
            migration.contains(expected),
            "compat gateway replay migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623073000_compat_gateway_replay_events;"));
    assert!(migrator.contains("Box::new(m20260623073000_compat_gateway_replay_events::Migration)"));
}
