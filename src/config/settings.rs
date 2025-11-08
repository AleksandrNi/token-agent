use serde::Deserialize;

/// ================================
/// Global service-wide settings
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct SettingsConfig {
    pub safety_margin_seconds: Option<u64>,
    pub retry: Option<RetryConfig>,
    pub metrics: MetricsConfig,
    pub server: ServerConfig,
    pub logging: Option<LoggingConfig>
}

#[derive(Debug, Deserialize, Clone)]
pub struct RetryConfig {
    pub attempts: Option<u32>,
    /// will be mutiply by 2 on every attempt until max_delay_ms 
    pub base_delay_ms: Option<u64>,
    /// max delay for retrying
    /// invariant: >= base_delay_ms. 
    /// used for token expiration time
    pub max_delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MetricsConfig {
    #[serde(default = "default_metrics_path")]
    pub path: String,
    #[serde(default)]
    pub is_enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    // pub path: Option<String>,
    pub host: String,
    pub port: String
}

/// ================================
/// Logging
/// ================================
#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String, // allowed: trace, debug, info, warn, error
    pub format: LogFormat,
}

impl LoggingConfig {
    pub fn new (level: String, format: LogFormat) -> Self {
        Self { level: level, format: format }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
    Compact,
}

impl LogFormat {
    pub fn from_env() -> Self {
        match std::env::var("LOG_FORMAT")
            .unwrap_or_else(|_| "json".to_string())
            .to_lowercase()
            .as_str()
        {
            "compact" | "text" => LogFormat::Compact,
            _ => LogFormat::Json,
        }
    }
}

fn default_metrics_path() -> String {
    "/metics".to_string()
}
