use std::fs;
use std::path::PathBuf;

fn repo_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(path).expect("repo file should be readable")
}

#[test]
fn webhook_schema_migration_defines_incoming_webhooks() {
    let migration = repo_file("src/db/migrations/m20260623060000_incoming_webhooks.rs");

    for expected in [
        "CREATE TABLE IF NOT EXISTS incoming_webhooks",
        "organization_id uuid NOT NULL REFERENCES organizations(id)",
        "space_id uuid NOT NULL REFERENCES spaces(id)",
        "channel_id uuid NOT NULL REFERENCES channels(id)",
        "bot_user_id uuid NOT NULL REFERENCES users(id)",
        "created_by_user_id uuid NOT NULL REFERENCES users(id)",
        "token_hash text NOT NULL UNIQUE",
        "token_last_four text NOT NULL",
        "CONSTRAINT incoming_webhooks_status_check",
        "CREATE INDEX IF NOT EXISTS idx_incoming_webhooks_channel",
        "CREATE INDEX IF NOT EXISTS idx_incoming_webhooks_organization",
    ] {
        assert!(
            migration.contains(expected),
            "incoming webhook migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623060000_incoming_webhooks;"));
    assert!(migrator.contains("Box::new(m20260623060000_incoming_webhooks::Migration)"));
}

#[test]
fn webhook_message_override_migration_extends_messages() {
    let migration = repo_file("src/db/migrations/m20260623074000_message_webhook_overrides.rs");

    for expected in [
        "ALTER TABLE messages",
        "ADD COLUMN IF NOT EXISTS webhook_username text NULL",
        "ADD COLUMN IF NOT EXISTS webhook_avatar_url text NULL",
    ] {
        assert!(
            migration.contains(expected),
            "message webhook override migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623074000_message_webhook_overrides;"));
    assert!(migrator.contains("Box::new(m20260623074000_message_webhook_overrides::Migration)"));
}

#[test]
fn webhook_policy_migration_defines_organization_override_controls() {
    let migration = repo_file("src/db/migrations/m20260623075000_organization_webhook_policies.rs");

    for expected in [
        "CREATE TABLE IF NOT EXISTS organization_webhook_policies",
        "organization_id uuid PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE",
        "allow_identity_overrides boolean NOT NULL DEFAULT true",
        "DROP TABLE IF EXISTS organization_webhook_policies",
    ] {
        assert!(
            migration.contains(expected),
            "webhook policy migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623075000_organization_webhook_policies;"));
    assert!(
        migrator.contains("Box::new(m20260623075000_organization_webhook_policies::Migration)")
    );
}
