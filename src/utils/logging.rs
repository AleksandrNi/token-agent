
use clap::ValueEnum;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use anyhow::Result;
use crate::ServiceConfig;
use crate::config::settings::{LogFormat, LoggingConfig};


#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogLevel {
    TRACE,
    DEBUG,
    INFO,
    WARN,
    ERROR,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match *self {
            LogLevel::TRACE => "TRACE",
            LogLevel::DEBUG => "DEBUG",
            LogLevel::INFO => "INFO",
            LogLevel::WARN => "WARN",
            LogLevel::ERROR => "ERROR",
        }
    }
}


pub async fn run(service_config: &ServiceConfig, arg_log_level: Option<LogLevel>) -> Result<()> {
    let logging_config = service_config
        .settings
        .logging
        .as_ref()
        .map(|config| {
            LoggingConfig::new(
                arg_log_level.to_owned()
                .map(|e|e.as_str().to_string())
                .ok_or(config.level.to_owned())
                .unwrap_or("info".to_owned()),
                
                config.format.to_owned(),
            )
        })
        .or(Some(LoggingConfig {
            level: "info".to_owned(),
            format: LogFormat::Compact,
        }))
        .unwrap();

    init_logging(&logging_config);
    Ok(())
}


/// Initialize tracing with the desired config.
pub fn init_logging(cfg: &LoggingConfig) {
    let env_filter = EnvFilter::try_new(&cfg.level)
        .unwrap_or_else(|_| EnvFilter::new("debug"));

    // Base layer: filter + writer
    let registry = tracing_subscriber::registry().with(env_filter);

    // Choose format layer
    match cfg.format {
        LogFormat::Json => {
            let layer = fmt::layer()
                .json()
                .with_timer(UtcTime::rfc_3339())
                .flatten_event(true) // flattens fields — good for CRI log parsers
                .with_ansi(false); // CRI parsers dislike ANSI color codes

            let _ = registry.with(layer).try_init();
        }
        LogFormat::Compact => {
            let layer = fmt::layer()
                .compact()
                .with_timer(UtcTime::rfc_3339())
                .with_ansi(true);

            let _ = registry.with(layer).try_init();
        }
    };
}