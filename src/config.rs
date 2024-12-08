use anyhow::Context;
use serde::Deserialize;
use std::fs;
use std::path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerConfig {
    pub listen_address: String,
    pub port: u16,
}

// read config file specified in command line
pub fn read_config(path: impl AsRef<path::Path>) -> Result<Config, anyhow::Error> {
    let path = path.as_ref();
    let templated_config_str = fs::read_to_string(path)
        .with_context(|| format!("Unable to read config file: {}", path.display()))?;

    let config: Config = toml::from_str(&templated_config_str)
        .with_context(|| format!("Unable to parse config file: {}", path.display()))?;

    Ok(config)
}
