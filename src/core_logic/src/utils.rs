use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub fn current_timestamp() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_millis() as u64
}

pub fn generate_custom_uuid() -> String {
    Uuid::new_v4().to_string()
}
