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
