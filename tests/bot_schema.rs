use std::fs;
use std::path::PathBuf;

fn repo_file(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(path).expect("repo file should be readable")
}

#[test]
fn bot_schema_migration_defines_applications_users_and_tokens() {
    let migration = repo_file("src/db/migrations/m20260623055000_bots.rs");

    for expected in [
        "CREATE TABLE IF NOT EXISTS bot_applications",
        "CREATE TABLE IF NOT EXISTS bot_tokens",
        "bot_user_id uuid NOT NULL REFERENCES users(id)",
        "organization_id uuid NOT NULL REFERENCES organizations(id)",
        "created_by_user_id uuid NOT NULL REFERENCES users(id)",
        "token_hash text NOT NULL UNIQUE",
        "token_last_four text NOT NULL",
        "CREATE INDEX IF NOT EXISTS idx_bot_applications_organization",
        "CREATE INDEX IF NOT EXISTS idx_bot_tokens_application",
    ] {
        assert!(
            migration.contains(expected),
            "bot migration should contain {expected}"
        );
    }

    let migrator = repo_file("src/db/migrations/mod.rs");
    assert!(migrator.contains("mod m20260623055000_bots;"));
    assert!(migrator.contains("Box::new(m20260623055000_bots::Migration)"));
}
