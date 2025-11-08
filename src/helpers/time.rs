use chrono::Utc;
use tokio::time::Instant;

use crate::config::sources::SAFETY_MARGIN_SECONDS_SOURCE_DEFAULT;

pub fn get_token_safety_margin_seconds(
    safety_margin_seconds_settings: Option<u64>,
    safety_margin_seconds_source: Option<u64>,
) -> u64 {
    // source level
    safety_margin_seconds_source
        // settings (global) level
        .or(safety_margin_seconds_settings)
        .or(Some(SAFETY_MARGIN_SECONDS_SOURCE_DEFAULT))
        .unwrap()
}

pub fn now_u64() -> u64 {
    now_i64() as u64
}

pub fn now_i64() -> i64 {
    Utc::now().timestamp()
}

pub fn get_instant() -> Instant {
    Instant::now()
}