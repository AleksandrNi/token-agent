use std::{path::Path};
use anyhow::{anyhow, Result};

use crate::ServiceConfig;
use crate::config::proc_loader::file_to_config;

pub async  fn run(config_path: &str) -> Result<ServiceConfig> {    
    let path = Path::new(config_path);
    file_to_config(path).await.map_err(|e| anyhow!(format!("Invalid config format: {}", e)))
}