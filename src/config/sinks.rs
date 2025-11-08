use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SinkType {
    File,
    /// not implemented yet
    Uds, 
    Http,
}

// used for passing event from sources to active sinks
#[derive(Clone, Debug)]
pub struct SinkMessage(pub String);

/// The top-level sink configuration block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SinkConfig {
    #[serde(default = "default_token_id")]
    pub sink_id: String,
    /// Type of sink: "file", "uds", or "http".
    #[serde(rename = "type")]
    pub sink_type: SinkType,

    /// Source ID from which token originates.
    /// Invariant: must exist in `sources`.
    pub source_id: String,

    /// Path or endpoint where the token will be propagated.
    /// - For `file`/`uds`: absolute filesystem path.
    /// - For `http`: relative URL path (e.g., `/tokens/client`).
    pub path: String,

    /// The ID of the token (defined in source.parse.tokens).
    pub token_id: String,

    /// Optional HTTP response definition (for type = "http").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<HttpResponseBlock>,
    
}

/// HTTP response structure for HTTP sinks.
///
/// Defines exactly what and how to serve in response:
/// - Headers can map static or token values
/// - Body defines JSON fields dynamically
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponseBlock {
    /// MIME type of the HTTP response, e.g., "application/json".
    #[serde(default = "default_content_type")]
    pub content_type: String,

    /// Optional header mappings (token, expiration, or static strings).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, ResponseField>>,

    /// Optional body mappings (for JSON response construction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<HashMap<String, ResponseField>>,
}

/// Represents a single response field (header or body).
///
/// Each field can either:
/// - Reference a token from cache
/// - Reference the token’s expiration value
/// - Contain a static literal string
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseField {
    /// Reference to a token ID (resolved from cache)
    Token {
        /// ID of the token to be rendered.
        #[serde(default = "default_token_id")]
        id: String,
    },

    /// Reference to expiration time.
    /// Invariant: value must be the expiration ID from source.
    Expiration {
        /// Expiration format (seconds, rfc3339, unix)
        #[serde(default)]
        format: ExpirationSinkFormat,
        /// ID of the token to be rendered.
        #[serde(default = "default_token_id")]
        id: String,
    },

    /// Literal string (static value)
    String {
        value: String,
    },
}

/// Supported expiration output formats.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExpirationSinkFormat {
    /// Duration in seconds until expiration.
    #[default]
    Seconds,

    /// RFC3339 timestamp format (e.g. "2025-10-07T10:00:00Z")
    Rfc3339,

    /// Unix timestamp (integer seconds since epoch)
    Unix,
}

fn default_content_type() -> String {
    "application/json".to_string()
}

fn default_token_id() -> String {
    "default_token_id".to_string()
}


