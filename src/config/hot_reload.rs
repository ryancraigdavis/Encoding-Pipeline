//! Configuration hot-reload functionality.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, RwLock};

use super::loader::load_and_validate;
use super::model::AppConfig;
use crate::validation::SystemCapabilities;

/// Watches the configuration file and triggers reloads on changes.
pub struct ConfigWatcher {
    config: Arc<RwLock<AppConfig>>,
    config_path: std::path::PathBuf,
    capabilities: SystemCapabilities,
    reload_tx: mpsc::Sender<ConfigReloadEvent>,
}

/// Events emitted by the configuration watcher.
#[derive(Debug, Clone)]
pub enum ConfigReloadEvent {
    /// Configuration was successfully reloaded.
    Reloaded,
    /// Configuration reload failed validation.
    ValidationFailed { error_count: usize },
}

impl ConfigWatcher {
    /// Creates a new configuration watcher.
    pub fn new(
        config: Arc<RwLock<AppConfig>>,
        config_path: &Path,
        capabilities: SystemCapabilities,
        reload_tx: mpsc::Sender<ConfigReloadEvent>,
    ) -> Self {
        Self {
            config,
            config_path: config_path.to_path_buf(),
            capabilities,
            reload_tx,
        }
    }

    /// Starts watching the configuration file for changes.
    pub async fn start(self) -> Result<()> {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default(),
        )?;

        watcher.watch(&self.config_path, RecursiveMode::NonRecursive)?;

        // Spawn the file change handler
        tokio::spawn(async move {
            self.handle_changes(rx).await;
        });

        Ok(())
    }

    /// Handles file change events with debouncing.
    async fn handle_changes(self, rx: std::sync::mpsc::Receiver<notify::Event>) {
        let debounce_duration = Duration::from_millis(500);
        let mut last_reload = std::time::Instant::now();

        loop {
            match rx.recv() {
                Ok(event) => {
                    if !event.kind.is_modify() {
                        continue;
                    }

                    // Debounce rapid changes
                    if last_reload.elapsed() < debounce_duration {
                        continue;
                    }

                    // Wait a bit for the file to be fully written
                    tokio::time::sleep(debounce_duration).await;

                    match self.try_reload().await {
                        Ok(()) => {
                            tracing::info!("Configuration reloaded successfully");
                            let _ = self.reload_tx.send(ConfigReloadEvent::Reloaded).await;
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Configuration reload failed");
                            let _ = self
                                .reload_tx
                                .send(ConfigReloadEvent::ValidationFailed { error_count: 1 })
                                .await;
                        }
                    }

                    last_reload = std::time::Instant::now();
                }
                Err(_) => {
                    tracing::warn!("Config watcher channel closed");
                    break;
                }
            }
        }
    }

    /// Attempts to reload and validate the configuration.
    async fn try_reload(&self) -> Result<()> {
        let new_config = load_and_validate(&self.config_path, &self.capabilities)?;

        // Swap the configuration atomically
        let mut config = self.config.write().await;
        *config = new_config;

        Ok(())
    }
}
