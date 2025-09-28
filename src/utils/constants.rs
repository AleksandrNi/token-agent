//! Shared constants and invariants

pub const DEFAULT_SAFETY_MARGIN_SECS: u64 = 60;
pub const DEFAULT_HTTP_TIMEOUT_MS: u64 = 5000;

// Supported source types
pub const SOURCE_HTTP: &str = "http";
pub const SOURCE_METADATA: &str = "metadata";
pub const SOURCE_OAUTH2: &str = "oauth2";