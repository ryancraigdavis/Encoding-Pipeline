//! Configuration loading, validation, and hot-reload management.

pub mod cache;
pub mod hot_reload;
pub mod loader;
pub mod model;

use std::path::Path;
use std::sync::{Arc, RwLock};

use anyhow::Result;

use crate::validation::SystemCapabilities;
pub use model::AppConfig;

/// Manages configuration loading, caching, and hot-reloading.
pub struct ConfigManager {
    config: Arc<RwLock<AppConfig>>,
    config_path: std::path::PathBuf,
}

impl ConfigManager {
    /// Creates a new ConfigManager by loading and validating the config file.
    pub async fn new(config_path: &Path, capabilities: &SystemCapabilities) -> Result<Self> {
        let config = loader::load_and_validate(config_path, capabilities)?;

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            config_path: config_path.to_path_buf(),
        })
    }

    /// Returns a thread-safe reference to the current configuration.
    pub fn get_config(&self) -> Arc<RwLock<AppConfig>> {
        Arc::clone(&self.config)
    }

    /// Returns the path to the configuration file.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }
}
