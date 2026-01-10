//! File size stability detection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Tracks file size stability to detect when files are fully written.
pub struct StabilityChecker {
    /// Duration the file size must remain stable.
    stability_duration: Duration,
    /// Interval between stability checks.
    poll_interval: Duration,
    /// Currently tracked files.
    tracked_files: HashMap<PathBuf, TrackedFile>,
    /// Channel to send ready files.
    ready_tx: mpsc::Sender<PathBuf>,
}

/// A file being tracked for stability.
struct TrackedFile {
    /// Last recorded file size.
    last_size: u64,
    /// When the file size became stable (None if still changing).
    stable_since: Option<Instant>,
    /// Profile name for this file.
    profile_name: String,
}

impl StabilityChecker {
    /// Creates a new stability checker.
    pub fn new(
        stability_duration: Duration,
        poll_interval: Duration,
        ready_tx: mpsc::Sender<PathBuf>,
    ) -> Self {
        Self {
            stability_duration,
            poll_interval,
            tracked_files: HashMap::new(),
            ready_tx,
        }
    }

    /// Starts tracking a file for stability.
    pub fn track(&mut self, path: PathBuf, profile_name: String) {
        if self.tracked_files.contains_key(&path) {
            debug!(?path, "File already being tracked");
            return;
        }

        let size = std::fs::metadata(&path)
            .map(|m| m.len())
            .unwrap_or(0);

        info!(?path, size, "Started tracking file for stability");

        self.tracked_files.insert(
            path,
            TrackedFile {
                last_size: size,
                stable_since: None,
                profile_name,
            },
        );
    }

    /// Stops tracking a file.
    pub fn untrack(&mut self, path: &Path) {
        self.tracked_files.remove(path);
    }

    /// Checks all tracked files for stability.
    pub async fn check_all(&mut self) {
        let mut ready_files = Vec::new();

        for (path, tracked) in &mut self.tracked_files {
            let current_size = match std::fs::metadata(path) {
                Ok(m) => m.len(),
                Err(e) => {
                    warn!(?path, error = %e, "Failed to get file metadata");
                    continue;
                }
            };

            if current_size == tracked.last_size && current_size > 0 {
                // Size is stable
                if tracked.stable_since.is_none() {
                    tracked.stable_since = Some(Instant::now());
                    debug!(?path, "File size became stable");
                }

                if let Some(stable_since) = tracked.stable_since {
                    if stable_since.elapsed() >= self.stability_duration {
                        info!(?path, "File is ready (stable for {:?})", self.stability_duration);
                        ready_files.push(path.clone());
                    }
                }
            } else {
                // Size changed, reset stability tracking
                if tracked.stable_since.is_some() {
                    debug!(?path, old_size = tracked.last_size, new_size = current_size, "File size changed, resetting stability");
                }
                tracked.last_size = current_size;
                tracked.stable_since = None;
            }
        }

        // Send ready files and remove from tracking
        for path in ready_files {
            self.tracked_files.remove(&path);
            if let Err(e) = self.ready_tx.send(path.clone()).await {
                warn!(?path, error = %e, "Failed to send ready file notification");
            }
        }
    }

    /// Returns the poll interval for this checker.
    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    /// Returns the number of files currently being tracked.
    pub fn tracked_count(&self) -> usize {
        self.tracked_files.len()
    }
}
