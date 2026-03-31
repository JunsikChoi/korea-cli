//! Configuration management — API key resolution (env → toml) and settings.

pub mod paths;

use anyhow::Result;
use serde::{Deserialize, Serialize};

const ENV_API_KEY: &str = "DATA_GO_KR_API_KEY";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub api_key: Option<String>,
    pub catalog_updated_at: Option<String>,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = paths::config_file()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = paths::config_file()?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Resolve API key: env var takes priority over config.toml.
    pub fn resolve_api_key(&self) -> Option<String> {
        std::env::var(ENV_API_KEY)
            .ok()
            .or_else(|| self.api_key.clone())
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "api-key" => self.api_key = Some(value.to_string()),
            _ => anyhow::bail!("Unknown config key: {key}"),
        }
        self.save()
    }

    pub fn get(&self, key: &str) -> Result<String> {
        match key {
            "api-key" => self
                .resolve_api_key()
                .ok_or_else(|| anyhow::anyhow!("api-key not set")),
            _ => anyhow::bail!("Unknown config key: {key}"),
        }
    }
}
