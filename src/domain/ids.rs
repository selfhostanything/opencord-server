use uuid::Uuid;

pub fn new_uuid_v7() -> Uuid {
    Uuid::now_v7()
}

pub fn new_prefixed_id(prefix: &str) -> String {
    format!("{}_{}", prefix.trim(), new_uuid_v7())
}
