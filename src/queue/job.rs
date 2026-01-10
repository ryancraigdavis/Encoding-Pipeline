//! Encoding job definitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Represents an encoding job in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeJob {
    /// Unique identifier for this job.
    pub id: String,

    /// Path to the source file.
    pub input_path: PathBuf,

    /// Path for the encoded output.
    pub output_path: PathBuf,

    /// Name of the profile to use for encoding.
    pub profile_name: String,

    /// Current status of the job.
    pub status: JobStatus,

    /// Number of encoding attempts made.
    pub attempt_count: u32,

    /// Timestamp when the job was created.
    pub created_at: DateTime<Utc>,

    /// Timestamp when the job was last updated.
    pub updated_at: DateTime<Utc>,

    /// Timestamp when encoding started (if applicable).
    pub started_at: Option<DateTime<Utc>>,

    /// Timestamp when encoding completed (if applicable).
    pub completed_at: Option<DateTime<Utc>>,

    /// Error message if the job failed.
    pub error_message: Option<String>,

    /// Encoding progress percentage (0-100).
    pub progress: Option<f32>,

    /// Metadata about the encode result.
    pub result_metadata: Option<EncodeResultMetadata>,
}

impl EncodeJob {
    /// Creates a new encoding job with the given parameters.
    pub fn new(input_path: PathBuf, output_path: PathBuf, profile_name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            input_path,
            output_path,
            profile_name,
            status: JobStatus::Pending,
            attempt_count: 0,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            error_message: None,
            progress: None,
            result_metadata: None,
        }
    }

    /// Marks the job as in progress.
    pub fn start(&mut self) {
        self.status = JobStatus::InProgress;
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self.attempt_count += 1;
    }

    /// Marks the job as completed successfully.
    pub fn complete(&mut self, metadata: EncodeResultMetadata) {
        self.status = JobStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self.progress = Some(100.0);
        self.result_metadata = Some(metadata);
    }

    /// Marks the job as failed.
    pub fn fail(&mut self, error: String) {
        self.status = JobStatus::Failed;
        self.updated_at = Utc::now();
        self.error_message = Some(error);
    }

    /// Marks the job for retry.
    pub fn retry(&mut self) {
        self.status = JobStatus::Pending;
        self.updated_at = Utc::now();
        self.error_message = None;
        self.progress = None;
    }

    /// Marks the job as moved to dead letter queue.
    pub fn dead_letter(&mut self, reason: String) {
        self.status = JobStatus::DeadLetter;
        self.updated_at = Utc::now();
        self.error_message = Some(reason);
    }

    /// Updates the progress of the job.
    pub fn update_progress(&mut self, progress: f32) {
        self.progress = Some(progress.clamp(0.0, 100.0));
        self.updated_at = Utc::now();
    }
}

/// Status of an encoding job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Job is waiting to be processed.
    Pending,
    /// Job is currently being encoded.
    InProgress,
    /// Job completed successfully.
    Completed,
    /// Job failed (may be retried).
    Failed,
    /// Job moved to dead letter queue after exhausting retries.
    DeadLetter,
}

/// Metadata about a completed encode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeResultMetadata {
    /// Original file size in bytes.
    pub input_size: u64,

    /// Encoded file size in bytes.
    pub output_size: u64,

    /// Encoding duration in seconds.
    pub encode_duration_secs: f64,

    /// VMAF score achieved (if measured).
    pub vmaf_score: Option<f32>,

    /// Video duration in seconds.
    pub video_duration_secs: f64,

    /// Encoding speed (e.g., 2.5x means 2.5 seconds of video per second of encoding).
    pub encoding_speed: f64,
}

impl EncodeResultMetadata {
    /// Calculates the compression ratio (input_size / output_size).
    pub fn compression_ratio(&self) -> f64 {
        if self.output_size == 0 {
            0.0
        } else {
            self.input_size as f64 / self.output_size as f64
        }
    }

    /// Calculates the size reduction percentage.
    pub fn size_reduction_percent(&self) -> f64 {
        if self.input_size == 0 {
            0.0
        } else {
            (1.0 - (self.output_size as f64 / self.input_size as f64)) * 100.0
        }
    }
}
