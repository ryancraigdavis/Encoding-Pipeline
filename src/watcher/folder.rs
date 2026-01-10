//! Individual folder watching.

use std::path::{Path, PathBuf};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::error::WatcherError;

/// Watches a single folder for new files.
pub struct FolderWatcher {
    /// Path to watch.
    watch_path: PathBuf,
    /// Whether to watch recursively.
    recursive: bool,
    /// File patterns to match.
    file_patterns: Vec<glob::Pattern>,
    /// Profile name for matched files.
    profile_name: String,
    /// Channel to send detected files.
    file_tx: mpsc::Sender<DetectedFile>,
}

/// A file detected by the watcher.
#[derive(Debug, Clone)]
pub struct DetectedFile {
    /// Path to the detected file.
    pub path: PathBuf,
    /// Name of the profile to use.
    pub profile_name: String,
}

impl FolderWatcher {
    /// Creates a new folder watcher.
    pub fn new(
        watch_path: PathBuf,
        recursive: bool,
        file_patterns: Vec<String>,
        profile_name: String,
        file_tx: mpsc::Sender<DetectedFile>,
    ) -> Result<Self, WatcherError> {
        let patterns: Result<Vec<_>, _> = file_patterns
            .iter()
            .map(|p| glob::Pattern::new(p))
            .collect();

        let patterns = patterns.map_err(|e| WatcherError::WatchFailed {
            path: watch_path.clone(),
            message: format!("Invalid file pattern: {}", e),
        })?;

        Ok(Self {
            watch_path,
            recursive,
            file_patterns: patterns,
            profile_name,
            file_tx,
        })
    }

    /// Starts watching the folder.
    pub async fn start(self) -> Result<(), WatcherError> {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default(),
        )?;

        let mode = if self.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        watcher.watch(&self.watch_path, mode)?;

        info!(path = ?self.watch_path, recursive = self.recursive, "Started watching folder");

        // Handle events in a separate task
        tokio::spawn(async move {
            self.handle_events(rx).await;
        });

        Ok(())
    }

    /// Scans the folder for existing files.
    pub async fn scan_existing(&self) -> Result<Vec<DetectedFile>, WatcherError> {
        let mut files = Vec::new();

        let walker = if self.recursive {
            walkdir::WalkDir::new(&self.watch_path)
        } else {
            walkdir::WalkDir::new(&self.watch_path).max_depth(1)
        };

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && self.matches_patterns(path) {
                files.push(DetectedFile {
                    path: path.to_path_buf(),
                    profile_name: self.profile_name.clone(),
                });
            }
        }

        info!(count = files.len(), path = ?self.watch_path, "Scanned existing files");
        Ok(files)
    }

    /// Handles file system events.
    async fn handle_events(self, rx: std::sync::mpsc::Receiver<Event>) {
        loop {
            match rx.recv() {
                Ok(event) => {
                    self.process_event(event).await;
                }
                Err(_) => {
                    warn!(path = ?self.watch_path, "Watcher channel closed");
                    break;
                }
            }
        }
    }

    /// Processes a single file system event.
    async fn process_event(&self, event: Event) {
        // We care about file creation and modification
        let dominated = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_)
        );

        if !dominated {
            return;
        }

        for path in event.paths {
            if !path.is_file() {
                continue;
            }

            if !self.matches_patterns(&path) {
                debug!(?path, "File does not match patterns");
                continue;
            }

            debug!(?path, "Detected new file");

            let detected = DetectedFile {
                path,
                profile_name: self.profile_name.clone(),
            };

            if let Err(e) = self.file_tx.send(detected).await {
                error!(error = %e, "Failed to send detected file");
            }
        }
    }

    /// Checks if a path matches any of the configured patterns.
    fn matches_patterns(&self, path: &Path) -> bool {
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        self.file_patterns.iter().any(|p| p.matches(filename))
    }
}
