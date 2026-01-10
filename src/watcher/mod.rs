//! File system watching for new video files.

pub mod folder;
pub mod manager;
pub mod stability;

pub use folder::FolderWatcher;
pub use manager::WatcherManager;
pub use stability::StabilityChecker;
