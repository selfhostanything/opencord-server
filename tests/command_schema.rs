use std::fs;
use std::path::PathBuf;

fn repo_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(path).expect("repo file should be readable")
}

#[test]
fn command_schema_migration_defines_application_commands_and_interactions() {
    let migration = repo_file("src/db/migrations/m20260623061000_commands.rs");

    for expected in [
        "CREATE TABLE IF NOT EXISTS application_commands",
        "CREATE TABLE IF NOT EXISTS command_interactions",
        "application_id uuid NOT NULL REFERENCES bot_applications(id)",
        "space_id uuid NOT NULL REFERENCES spaces(id)",
        "channel_id uuid NOT NULL REFERENCES channels(id)",
        "command_id uuid NOT NULL REFERENCES application_commands(id)",
        "token_hash text NOT NULL UNIQUE",
        "CONSTRAINT application_commands_status_check",
        "CONSTRAINT command_interactions_status_check",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_application_commands_unique_name",
        "CREATE INDEX IF NOT EXISTS idx_command_interactions_command",
    ] {
        assert!(
            migration.contains(expected),
            "command migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623061000_commands;"));
    assert!(migrator.contains("Box::new(m20260623061000_commands::Migration)"));
}

#[test]
fn global_application_commands_migration_adds_global_scope() {
    let migration = repo_file("src/db/migrations/m20260623080500_global_application_commands.rs");

    for expected in [
        "DROP INDEX IF EXISTS idx_application_commands_unique_name",
        "ALTER COLUMN space_id DROP NOT NULL",
        "idx_application_commands_unique_space_name",
        "WHERE status = 'active' AND space_id IS NOT NULL",
        "idx_application_commands_unique_global_name",
        "WHERE status = 'active' AND space_id IS NULL",
        "DELETE FROM application_commands",
        "ALTER COLUMN space_id SET NOT NULL",
    ] {
        assert!(
            migration.contains(expected),
            "global command migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623080500_global_application_commands;"));
    assert!(migrator.contains("Box::new(m20260623080500_global_application_commands::Migration)"));
}

#[test]
fn interaction_followup_messages_migration_tracks_followup_ownership() {
    let migration = repo_file("src/db/migrations/m20260623081000_interaction_followup_messages.rs");

    for expected in [
        "CREATE TABLE IF NOT EXISTS interaction_followup_messages",
        "interaction_id uuid NOT NULL REFERENCES command_interactions(id) ON DELETE CASCADE",
        "message_id uuid NOT NULL REFERENCES messages(id) ON DELETE CASCADE",
        "PRIMARY KEY (interaction_id, message_id)",
        "CONSTRAINT interaction_followup_messages_unique_message UNIQUE (message_id)",
        "CREATE INDEX IF NOT EXISTS idx_interaction_followup_messages_interaction",
    ] {
        assert!(
            migration.contains(expected),
            "interaction follow-up migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623081000_interaction_followup_messages;"));
    assert!(
        migrator.contains("Box::new(m20260623081000_interaction_followup_messages::Migration)")
    );
}

#[test]
fn component_interactions_migration_extends_interaction_schema() {
    let migration = repo_file("src/db/migrations/m20260623065000_component_interactions.rs");

    for expected in [
        "ADD COLUMN IF NOT EXISTS interaction_type integer NOT NULL DEFAULT 2",
        "ADD COLUMN IF NOT EXISTS message_id uuid NULL REFERENCES messages(id) ON DELETE CASCADE",
        "ADD COLUMN IF NOT EXISTS custom_id text NULL",
        "ADD COLUMN IF NOT EXISTS component_type integer NULL",
        "ALTER COLUMN command_id DROP NOT NULL",
        "CONSTRAINT command_interactions_type_check",
        "interaction_type = 3",
        "CREATE INDEX IF NOT EXISTS idx_command_interactions_message",
        "CREATE INDEX IF NOT EXISTS idx_command_interactions_application_type",
    ] {
        assert!(
            migration.contains(expected),
            "component interaction migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623065000_component_interactions;"));
    assert!(migrator.contains("Box::new(m20260623065000_component_interactions::Migration)"));
}

#[test]
fn deferred_interactions_migration_extends_status_schema() {
    let migration = repo_file("src/db/migrations/m20260623070000_deferred_interactions.rs");

    for expected in [
        "DROP CONSTRAINT IF EXISTS command_interactions_status_check",
        "CHECK (status IN ('pending', 'deferred', 'responded', 'expired'))",
        "WHERE status = 'deferred'",
        "CHECK (status IN ('pending', 'responded', 'expired'))",
    ] {
        assert!(
            migration.contains(expected),
            "deferred interaction migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623070000_deferred_interactions;"));
    assert!(migrator.contains("Box::new(m20260623070000_deferred_interactions::Migration)"));
}

#[test]
fn interaction_response_messages_migration_tracks_original_response() {
    let migration = repo_file("src/db/migrations/m20260623071000_interaction_response_messages.rs");

    for expected in [
        "ADD COLUMN IF NOT EXISTS response_message_id uuid NULL REFERENCES messages(id) ON DELETE SET NULL",
        "CREATE INDEX IF NOT EXISTS idx_command_interactions_response_message",
        "WHERE response_message_id IS NOT NULL",
        "DROP COLUMN IF EXISTS response_message_id",
    ] {
        assert!(
            migration.contains(expected),
            "interaction response message migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623071000_interaction_response_messages;"));
    assert!(
        migrator.contains("Box::new(m20260623071000_interaction_response_messages::Migration)")
    );
}
