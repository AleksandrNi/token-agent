use crate::config::types::ServiceConfig;
use std::fs;
use std::path::Path;
use anyhow::{Result, bail};

/// Load and validate config from YAML file
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<ServiceConfig> {
    let raw = fs::read_to_string(path)?;
    let mut config: ServiceConfig = serde_yaml::from_str(&raw)?;

    // Apply defaults
    if config.logging.log_level.is_none() {
        config.logging.log_level = Some("info".into());
    }
    if config.settings.safety_margin_seconds.is_none() {
        config.settings.safety_margin_seconds = Some(60);
    }

    // Validate sources
    for (name, src) in &config.sources {
        if src.source_type != "http"
            && src.source_type != "metadata"
            && src.source_type != "oauth2"
        {
            bail!("Unsupported source type '{}' in {}", src.source_type, name);
        }
    }

    // Validate sinks
    for (name, sink) in &config.sinks {
        if sink.sink_type != "file" && sink.sink_type != "metrics" {
            bail!("Unsupported sink type '{}' in {}", sink.sink_type, name);
        }
    }

    Ok(config)
}