use opencord_server::domain::ids;

#[test]
fn new_uuid_v7_generates_version_7_uuid() {
    let id = ids::new_uuid_v7();

    assert_eq!(id.get_version_num(), 7);
}

#[test]
fn prefixed_id_uses_uuid_v7_suffix() {
    let id = ids::new_prefixed_id("org");

    assert!(id.as_str().starts_with("org_"));
    let uuid = id.as_str().trim_start_matches("org_");
    assert_eq!(uuid::Uuid::parse_str(uuid).unwrap().get_version_num(), 7);
}
