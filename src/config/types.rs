use serde::Deserialize;
use std::collections::HashMap;

/// ================================
/// Full service configuration
/// ================================
#[derive(Debug, Deserialize)]
pub struct ServiceConfig {
    pub settings: SettingsConfig,
    pub logging: LoggingConfig,
    pub sources: HashMap<String, SourceConfig>,
    pub sinks: HashMap<String, SinkConfig>,
    pub endpoints: HashMap<String, EndpointConfig>,
}

/// ================================
/// Global service-wide settings
/// ================================
#[derive(Debug, Deserialize)]
pub struct SettingsConfig {
    pub safety_margin_seconds: Option<u64>,
    pub retry: Option<RetryConfig>,
    pub observability: Option<ObservabilityConfig>,
    pub server: Option<ServerConfig>,
}

#[derive(Debug, Deserialize)]
pub struct RetryConfig {
    pub attempts: Option<u32>,
    pub base_delay_ms: Option<u64>,
    pub max_delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ObservabilityConfig {
    pub metrics_endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub uds_path: Option<String>,
}

/// ================================
/// Logging
/// ================================
#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    pub log_level: Option<String>, // invariants: [trace, debug, info, warn, error]
}

/// ================================
/// Sources
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct SourceConfig {
    #[serde(rename = "type")]
    pub source_type: String, // e.g., "http", "metadata", "oauth2"
    pub request: Option<RequestConfig>,
    pub parse: Option<ParseConfig>,
    pub inputs: Option<Vec<String>>,
    pub safety_margin_seconds: Option<u64>,
}

/// HTTP request details
#[derive(Debug, Deserialize, Clone)]
pub struct RequestConfig {
    pub url: String,
    pub method: String, // invariants: GET, POST
    pub headers: Option<HashMap<String, HeaderValue>>,
    pub body: Option<HashMap<String, BodyValue>>,
}

/// Header value sources
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum HeaderValue {
    Literal {
        value: String,
    },
    FromEnv {
        from_env: String,
    },
    FromFile {
        from_file: String,
    },
    Ref {
        source: String,
        field: String,
        prefix: Option<String>,
    },
}

/// Body value sources
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum BodyValue {
    Literal { value: String },
    Ref { source: String, field: String },
}

/// ================================
/// Parsing
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct ParseConfig {
    pub fields: Vec<ParseField>,
}

/// Field with parent-awareness
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "from")] // "body", "header", "status"
pub enum ParseField {
    Body {
        name: String,
        pointer: String,
        #[serde(default)]
        r#type: Option<String>,
        #[serde(default)]
        default: Option<String>,
        kind: FieldKind,
    },
    Header {
        name: String,
        key: String,
        #[serde(default)]
        r#type: Option<String>,
        #[serde(default)]
        default: Option<String>,
        kind: FieldKind,
    },
    Status {
        name: String,
        #[serde(default)]
        expected: Option<u16>,
        #[serde(default)]
        default: Option<String>,
        kind: FieldKind,
    },
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Token,
    Expiration,
    Other,
}

impl ParseField {
    pub fn parent(&self) -> &str {
        match self {
            ParseField::Body { .. } => "body",
            ParseField::Header { .. } => "header",
            ParseField::Status { .. } => "status",
        }
    }

    pub fn field_name(&self) -> &str {
        match self {
            ParseField::Body { name, .. } => name,
            ParseField::Header { name, .. } => name,
            ParseField::Status { name, .. } => name,
        }
    }
    pub fn kind(&self) -> FieldKind {
        match self {
            ParseField::Body { kind, .. } => kind.to_owned(),
            ParseField::Header { kind, .. } => kind.to_owned(),
            ParseField::Status { kind, .. } => kind.to_owned(),
        }
    }
}

/// ================================
/// Sinks
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct SinkConfig {
    #[serde(rename = "type")]
    pub sink_type: String, // e.g., "file", "metrics"
    pub path: Option<String>,
    pub watch: Option<bool>,
    pub inputs: Option<Vec<String>>,
}

/// ================================
/// Endpoints
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct EndpointConfig {
    #[serde(rename = "type")]
    pub endpoint_type: String, // e.g., "uds", "http"
    pub path: Option<String>,
    pub port: Option<u16>,
    pub inputs: Option<Vec<String>>,
}
