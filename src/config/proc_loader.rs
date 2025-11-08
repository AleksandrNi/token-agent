use std::{fs, path::Path};
use crate::config::proc_initiateor::initiate_default_values;
use crate::config::settings::{LogFormat, LoggingConfig};
use crate::config::sources::ServiceConfig;
use crate::observability::metrics::get_metrics;
use anyhow::{Result};
use regex::Regex;
use tracing::{debug, error};
use crate::config::proc_validator;

/// Load and validate config from YAML file
pub async  fn file_to_config(path: &Path) -> Result<ServiceConfig> {
    let content= fs::read_to_string(path)?;
        
    let expanded = expand_env_vars(&content);
     parse_config(expanded).await
}
pub async fn parse_config(content: String) -> Result<ServiceConfig> {
    let metrics = get_metrics().await;
    let mut service_config: ServiceConfig = serde_yaml::from_str(&content)
        .inspect_err(|e| {
            error!("parse config error: {}", e);
            metrics.parse_failures.inc();
        })?;

    // Apply defaults
    if service_config.settings.logging.is_none() {
        service_config.settings.logging = Some(LoggingConfig{level: "info".to_owned(), format: LogFormat::Compact});
    }
    if service_config.settings.safety_margin_seconds.is_none() {
        service_config.settings.safety_margin_seconds = Some(60);
    }
    service_config = initiate_default_values(service_config);
    debug!("validation config ...");
    let _ = proc_validator::validate_service_config(&service_config).await;
    
    Ok(service_config)
}

fn expand_env_vars(input: &str) -> String {
    let re = Regex::new(r"\$\{(\w+)(?::([^\}]+))?\}").unwrap();
    re.replace_all(input, |caps: &regex::Captures| {
        let var = &caps[1];
        let default = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        std::env::var(var).unwrap_or_else(|_| default.to_string())
    })
    .to_string()
}

