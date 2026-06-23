#[test]
fn host_run_service_targets_use_local_database_url() {
    let makefile = std::fs::read_to_string("Makefile").expect("read Makefile");

    for target in ["dev-api", "dev-realtime", "dev-worker"] {
        let command = make_target_command(&makefile, target);
        assert!(
            command.contains("DATABASE_URL=\"$(DATABASE_URL)\""),
            "{target} should pass DATABASE_URL so local services share the migrated database"
        );
    }
}

#[test]
fn local_api_target_allows_vite_browser_origins() {
    let makefile = std::fs::read_to_string("Makefile").expect("read Makefile");
    let command = make_target_command(&makefile, "dev-api");

    assert!(
        command.contains("OPENCORD_ALLOWED_ORIGINS=\"$(OPENCORD_DEV_ALLOWED_ORIGINS)\""),
        "dev-api should allow the local web client origin for real browser smoke tests"
    );
    assert!(
        makefile.contains("http://localhost:5173,http://127.0.0.1:5173"),
        "local web development origins should be documented in the Makefile default"
    );
}

fn make_target_command(makefile: &str, target: &str) -> String {
    let marker = format!("{target}:\n");
    let (_, after_target) = makefile
        .split_once(&marker)
        .unwrap_or_else(|| panic!("{target} target exists"));

    after_target
        .lines()
        .take_while(|line| line.starts_with('\t') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
