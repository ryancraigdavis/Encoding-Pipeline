//! Manages multiple folder watchers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};

use super::folder::{DetectedFile, FolderWatcher};
use super::stability::StabilityChecker;
use crate::config::model::{AppConfig, Profile};
use crate::error::WatcherError;
use crate::queue::job::EncodeJob;
use crate::queue::QueueManager;

/// Manages all folder watchers and coordinates file detection.
pub struct WatcherManager {
    /// Active folder watchers by profile name.
    watchers: HashMap<String, ()>,
    /// Channel for detected files.
    file_rx: mpsc::Receiver<DetectedFile>,
    /// Channel sender for detected files (cloned to watchers).
    file_tx: mpsc::Sender<DetectedFile>,
    /// Stability checker for detected files.
    stability_checker: StabilityChecker,
    /// Channel for files ready to encode.
    ready_rx: mpsc::Receiver<PathBuf>,
    /// Queue manager for adding jobs.
    queue: QueueManager,
    /// Current configuration.
    config: Arc<RwLock<AppConfig>>,
}

impl WatcherManager {
    /// Creates a new watcher manager.
    pub async fn new(
        config: Arc<RwLock<AppConfig>>,
        queue: QueueManager,
        stability_duration: Duration,
        poll_interval: Duration,
    ) -> Self {
        let (file_tx, file_rx) = mpsc::channel(100);
        let (ready_tx, ready_rx) = mpsc::channel(100);

        let stability_checker = StabilityChecker::new(stability_duration, poll_interval, ready_tx);

        Self {
            watchers: HashMap::new(),
            file_rx,
            file_tx,
            stability_checker,
            ready_rx,
            queue,
            config,
        }
    }

    /// Starts watching all configured folders.
    pub async fn start(&mut self, process_existing: bool) -> Result<(), WatcherError> {
        let config = self.config.read().await;

        for profile in &config.profiles {
            self.add_watcher(profile).await?;

            if process_existing {
                self.scan_existing(profile).await?;
            }
        }

        drop(config);

        // Start the main event loop
        self.run_loop().await;

        Ok(())
    }

    /// Adds a watcher for a profile.
    async fn add_watcher(&mut self, profile: &Profile) -> Result<(), WatcherError> {
        let watcher = FolderWatcher::new(
            profile.input_path.clone(),
            profile.recursive,
            profile.file_patterns.clone(),
            profile.name.clone(),
            self.file_tx.clone(),
        )?;

        watcher.start().await?;
        self.watchers.insert(profile.name.clone(), ());

        info!(profile = %profile.name, path = ?profile.input_path, "Added folder watcher");
        Ok(())
    }

    /// Scans existing files for a profile.
    async fn scan_existing(&mut self, profile: &Profile) -> Result<(), WatcherError> {
        let watcher = FolderWatcher::new(
            profile.input_path.clone(),
            profile.recursive,
            profile.file_patterns.clone(),
            profile.name.clone(),
            self.file_tx.clone(),
        )?;

        let files = watcher.scan_existing().await?;

        for file in files {
            self.stability_checker
                .track(file.path, file.profile_name);
        }

        Ok(())
    }

    /// Runs the main event loop.
    async fn run_loop(&mut self) {
        let poll_interval = self.stability_checker.poll_interval();

        loop {
            tokio::select! {
                // Handle newly detected files
                Some(detected) = self.file_rx.recv() => {
                    self.stability_checker.track(detected.path, detected.profile_name);
                }

                // Handle files ready for encoding
                Some(path) = self.ready_rx.recv() => {
                    if let Err(e) = self.enqueue_file(path.clone()).await {
                        error!(?path, error = %e, "Failed to enqueue file");
                    }
                }

                // Periodic stability checks
                _ = tokio::time::sleep(poll_interval) => {
                    self.stability_checker.check_all().await;
                }
            }
        }
    }

    /// Enqueues a file for encoding.
    async fn enqueue_file(&mut self, path: PathBuf) -> Result<(), WatcherError> {
        let config = self.config.read().await;

        // Find the profile for this file
        let profile = config
            .profiles
            .iter()
            .find(|p| path.starts_with(&p.input_path));

        let profile = match profile {
            Some(p) => p,
            None => {
                error!(?path, "No profile found for file");
                return Ok(());
            }
        };

        // Calculate output path
        let output_path = calculate_output_path(&path, profile);

        let job = EncodeJob::new(path.clone(), output_path, profile.name.clone());

        drop(config);

        self.queue.enqueue(&job).await.map_err(|e| WatcherError::WatchFailed {
            path,
            message: format!("Failed to enqueue: {}", e),
        })?;

        info!(job_id = %job.id, "Enqueued encoding job");
        Ok(())
    }

    /// Reloads watchers after configuration change.
    pub async fn reload(&mut self) -> Result<(), WatcherError> {
        // For now, just log. Full implementation would diff configs
        // and add/remove watchers as needed.
        info!("Reloading watcher configuration");
        Ok(())
    }
}

/// Calculates the output path for a file based on profile settings.
fn calculate_output_path(input_path: &PathBuf, profile: &Profile) -> PathBuf {
    use crate::config::model::{FilenameMode, OutputStructure};

    let relative_path = input_path
        .strip_prefix(&profile.input_path)
        .unwrap_or(input_path.as_path());

    let mut output_path = profile.output_path.clone();

    match profile.output_naming.structure {
        OutputStructure::Mirror => {
            if let Some(parent) = relative_path.parent() {
                output_path = output_path.join(parent);
            }
        }
        OutputStructure::Flat => {
            // Just use the output directory directly
        }
    }

    let filename = match profile.output_naming.filename {
        FilenameMode::Preserve => {
            let mut name = relative_path
                .file_stem()
                .unwrap_or_default()
                .to_os_string();

            if let Some(suffix) = &profile.output_naming.suffix {
                name.push(suffix);
            }

            name.push(".mkv");
            PathBuf::from(name)
        }
        FilenameMode::Template => {
            // Template processing would go here
            // For now, fall back to preserve
            let name = relative_path.file_name().unwrap_or_default();
            PathBuf::from(name)
        }
    };

    output_path.join(filename)
}
