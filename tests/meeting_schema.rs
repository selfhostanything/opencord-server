use std::fs;
use std::path::PathBuf;

fn repo_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(path).expect("repo file should be readable")
}

#[test]
fn meeting_schema_migration_defines_required_tables_and_indexes() {
    let migration = repo_file("src/db/migrations/m20260623041000_meetings.rs");

    for expected in [
        "CREATE TABLE IF NOT EXISTS meetings",
        "CREATE TABLE IF NOT EXISTS meeting_attendees",
        "CREATE TABLE IF NOT EXISTS meeting_reminders",
        "CHECK (ends_at > starts_at)",
        "CHECK (user_id IS NOT NULL OR email IS NOT NULL)",
        "CHECK (offset_minutes >= 0)",
        "CREATE INDEX IF NOT EXISTS idx_meetings_organization_start",
        "CREATE INDEX IF NOT EXISTS idx_meeting_attendees_user",
        "CREATE INDEX IF NOT EXISTS idx_meeting_reminders_due",
    ] {
        assert!(
            migration.contains(expected),
            "meeting migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623041000_meetings;"));
    assert!(migrator.contains("Box::new(m20260623041000_meetings::Migration)"));
}
