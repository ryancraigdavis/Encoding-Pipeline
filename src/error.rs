//! Error types for the encoding pipeline.

use std::path::PathBuf;
use thiserror::Error;

/// Top-level application errors.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    #[error("Queue error: {0}")]
    Queue(#[from] QueueError),

    #[error("Encoder error: {0}")]
    Encoder(#[from] EncoderError),

    #[error("Watcher error: {0}")]
    Watcher(#[from] WatcherError),

    #[error("Notification error: {0}")]
    Notification(#[from] NotificationError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Configuration loading and parsing errors.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file '{path}': {source}")]
    ReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse config file '{path}': {message}")]
    ParseFailed { path: PathBuf, message: String },

    #[error("Config validation failed with {error_count} error(s)")]
    ValidationFailed { error_count: usize },

    #[error("Failed to cache config in Redis: {0}")]
    CacheFailed(String),
}

/// Configuration validation errors.
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Schema validation failed: {0}")]
    Schema(String),

    #[error("Semantic validation failed: {0}")]
    Semantic(String),

    #[error("Path validation failed: {0}")]
    Path(String),

    #[error("Codec '{codec}' is not available on this system")]
    CodecUnavailable { codec: String },

    #[error("Encoder '{encoder}' is not available on this system")]
    EncoderUnavailable { encoder: String },
}

/// Redis queue operation errors.
#[derive(Error, Debug)]
pub enum QueueError {
    #[error("Failed to connect to Redis at '{url}': {message}")]
    ConnectionFailed { url: String, message: String },

    #[error("Failed to enqueue job: {0}")]
    EnqueueFailed(String),

    #[error("Failed to dequeue job: {0}")]
    DequeueFailed(String),

    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: String },

    #[error("Failed to serialize job: {0}")]
    SerializationFailed(String),
}

/// Encoding operation errors.
#[derive(Error, Debug)]
pub enum EncoderError {
    #[error("av1an failed with exit code {code}: {stderr}")]
    Av1anFailed { code: i32, stderr: String },

    #[error("FFmpeg failed with exit code {code}: {stderr}")]
    FfmpegFailed { code: i32, stderr: String },

    #[error("mkvmerge failed with exit code {code}: {stderr}")]
    MkvmergeFailed { code: i32, stderr: String },

    #[error("Process spawn failed: {0}")]
    SpawnFailed(String),

    #[error("Encoding timed out after {seconds} seconds")]
    Timeout { seconds: u64 },

    #[error("Output verification failed: {0}")]
    VerificationFailed(String),
}

/// File watcher errors.
#[derive(Error, Debug)]
pub enum WatcherError {
    #[error("Failed to watch directory '{path}': {message}")]
    WatchFailed { path: PathBuf, message: String },

    #[error("File stability check failed for '{path}': {message}")]
    StabilityCheckFailed { path: PathBuf, message: String },

    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),
}

/// Notification sending errors.
#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("Discord webhook failed: {0}")]
    DiscordFailed(String),

    #[error("Prometheus metrics export failed: {0}")]
    PrometheusFailed(String),

    #[error("HTTP request failed: {0}")]
    HttpFailed(#[from] reqwest::Error),
}

/// Capability detection errors.
#[derive(Error, Debug)]
pub enum CapabilityError {
    #[error("Failed to run '{command}': {message}")]
    CommandFailed { command: String, message: String },

    #[error("Failed to parse encoder output: {0}")]
    ParseFailed(String),

    #[error("Required tool '{tool}' not found in PATH")]
    ToolNotFound { tool: String },
}
