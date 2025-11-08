use http::Method;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::config::{settings::SettingsConfig, sinks::SinkConfig};


/// ================================
/// Full service configuration
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct ServiceConfig {
    pub settings: SettingsConfig,
    pub sources: HashMap<String, SourceConfig>,
    pub sinks: HashMap<String, SinkConfig>,
}

/// ================================
/// Sources
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct SourceConfig {
    #[serde(rename = "type")]
    pub source_type: SourceTypes, // e.g., http, oauth2, metadata
    pub request: RequestConfig,
    pub parse: ParseConfig,
    pub inputs: Option<Vec<String>>,
    pub safety_margin_seconds: Option<u64>,
}

/// HTTP request details
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub struct RequestConfig {
    pub url: String,
    #[serde(with = "http_serde::method")]
    pub method: Method, // GET, POST
    pub headers: Option<HashMap<String, GenericSourceValue>>,
    pub body: Option<HashMap<String, GenericSourceValue>>,
    pub form: Option<FormValue>
}

/// Header value sources
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum GenericSourceValue {
    Literal {
        value: String,
    },
    FromEnv {
        from_env: String,
    },
    FromFile {
        path: String,
    },
    Ref {
        source: String,
        id: String,
        prefix: Option<String>,
    },
    Template {
        template: String,
        #[serde(default)]
        required: bool, // if true → fail-fast if template cannot be rendered
    },
}

/// Body value sources
#[derive(Debug, Deserialize, Clone)]
pub struct  FormValue {
    pub client_id: GenericSourceValue,
    pub client_secret: GenericSourceValue,
    pub scope: GenericSourceValue,
}

/// ================================
/// Parsing - Tokens & Expirations
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct ParseConfig {
    pub tokens: Vec<TokenField>,
}

pub const SAFETY_MARGIN_SECONDS_SOURCE_DEFAULT: u64 = 10;
/// Represents a token or expiration field
#[derive(Debug, Deserialize, Clone)]
pub struct TokenField {
    pub id: String,            // unique per source
    pub parent: String,        // one of: body, header, query
    pub pointer: String,       // JSON pointer or header key
    pub token_type: TokenType, // allowed: jwt, plain_text, expiration
    pub expiration: Option<Expiration>, // None for JWT, Some for plain or manual
                               // invariants documented in YAML contract
}

/// Expiration definition
#[derive(Debug, Deserialize, Clone)]
pub struct Expiration {
    pub source: ExpirationSource,        // self | field | manual
    pub pointer: Option<String>,         // required if source=field
    pub linked_token_id: Option<String>, // required if source=field
    pub manual_ttl_seconds: Option<u64>, // required if source=manual
    pub format: ExpirationSourceFormat
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExpirationSourceFormat {
    /// Duration in seconds until expiration.
    #[default]
    Seconds,

    /// Unix timestamp (integer seconds since epoch)
    Unix,
}

/// Token types
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Jwt,
    PlainText,
    // Expiration,
}

/// Expiration sources
#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ExpirationSource {
    #[serde(rename = "self")]
    SelfField, // extract from JWT
    JsonBodyField,  // extract from JSON Body field
    HeaderField,  // extract from Headers field
    Manual, // user-defined TTL
}

#[derive(Debug, Deserialize, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceTypes {
    HTTP,
    METADATA,
    OAUTH2,
}

// jwt oken
#[derive(Debug, Deserialize)]
pub struct JwtClaims {
    pub exp: u64,
}
