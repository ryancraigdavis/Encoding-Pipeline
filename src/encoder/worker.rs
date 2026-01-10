//! Encoding worker that processes jobs from the queue.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

use super::{av1an, ffmpeg, mkvmerge};
use crate::config::model::{AppConfig, Profile};
use crate::error::EncoderError;
use crate::media::{audio, probe, subtitle};
use crate::queue::dead_letter::{DeadLetterHandler, FailureAction};
use crate::queue::job::{EncodeJob, EncodeResultMetadata};
use crate::queue::QueueManager;

/// Worker that processes encoding jobs from the queue.
pub struct EncodeWorker {
    /// Queue manager for fetching and updating jobs.
    queue: QueueManager,
    /// Current configuration.
    config: Arc<RwLock<AppConfig>>,
    /// Maximum retry attempts.
    max_attempts: u32,
    /// Channel for progress updates.
    progress_tx: Option<mpsc::Sender<WorkerProgress>>,
}

/// Progress update from the worker.
#[derive(Debug, Clone)]
pub struct WorkerProgress {
    /// Job ID.
    pub job_id: String,
    /// Encoding progress percentage.
    pub percent: f32,
    /// Current phase.
    pub phase: EncodePhase,
}

/// Current phase of encoding.
#[derive(Debug, Clone)]
pub enum EncodePhase {
    /// Analyzing source file.
    Analyzing,
    /// Encoding video.
    EncodingVideo,
    /// Processing audio.
    ProcessingAudio,
    /// Extracting subtitles.
    ExtractingSubtitles,
    /// Muxing final output.
    Muxing,
    /// Verifying output.
    Verifying,
}

impl EncodeWorker {
    /// Creates a new encode worker.
    pub fn new(
        queue: QueueManager,
        config: Arc<RwLock<AppConfig>>,
        max_attempts: u32,
        progress_tx: Option<mpsc::Sender<WorkerProgress>>,
    ) -> Self {
        Self {
            queue,
            config,
            max_attempts,
            progress_tx,
        }
    }

    /// Runs the worker loop, processing jobs from the queue.
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting encode worker");

        loop {
            // Try to get a job from the queue
            match self.queue.dequeue().await {
                Ok(Some(mut job)) => {
                    info!(job_id = %job.id, input = ?job.input_path, "Processing job");

                    match self.process_job(&mut job).await {
                        Ok(()) => {
                            info!(job_id = %job.id, "Job completed successfully");
                            self.queue.complete_job(&job).await?;
                        }
                        Err(e) => {
                            error!(job_id = %job.id, error = %e, "Job failed");
                            self.handle_failure(job, e.to_string()).await?;
                        }
                    }
                }
                Ok(None) => {
                    // No jobs in queue, wait before checking again
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
                Err(e) => {
                    error!(error = %e, "Failed to dequeue job");
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                }
            }
        }
    }

    /// Processes a single encoding job.
    async fn process_job(&mut self, job: &mut EncodeJob) -> Result<(), EncoderError> {
        job.start();
        self.queue.update_job(job).await.ok();

        let config = self.config.read().await;
        let profile = config
            .profiles
            .iter()
            .find(|p| p.name == job.profile_name)
            .ok_or_else(|| EncoderError::SpawnFailed(format!(
                "Profile '{}' not found",
                job.profile_name
            )))?
            .clone();
        drop(config);

        // Create temp directory for this job
        let temp_dir = std::env::temp_dir().join(format!("encode_{}", job.id));
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

        let result = self.run_encode_pipeline(job, &profile, &temp_dir).await;

        // Clean up temp directory
        if let Err(e) = std::fs::remove_dir_all(&temp_dir) {
            warn!(error = %e, "Failed to clean up temp directory");
        }

        result
    }

    /// Runs the full encoding pipeline.
    async fn run_encode_pipeline(
        &mut self,
        job: &mut EncodeJob,
        profile: &Profile,
        temp_dir: &PathBuf,
    ) -> Result<(), EncoderError> {
        let start_time = std::time::Instant::now();

        // Phase 1: Analyze source
        self.send_progress(job, 0.0, EncodePhase::Analyzing).await;
        let probe_result = probe::probe(&job.input_path)
            .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

        // Phase 2: Determine audio and subtitle handling
        let audio_decisions = audio::process_audio_streams(&probe_result.audio_streams, &profile.audio);
        let subtitle_decisions = subtitle::process_subtitle_streams(&probe_result.subtitle_streams, &profile.subtitles);

        // Phase 3: Extract subtitles
        self.send_progress(job, 5.0, EncodePhase::ExtractingSubtitles).await;
        let extracted_subs = ffmpeg::extract_subtitles(&job.input_path, temp_dir, &subtitle_decisions).await?;

        // Check if we need to burn in subtitles
        let burn_in_sub = extracted_subs.iter().find(|s| s.should_burn_in);

        // Phase 4: Encode video
        self.send_progress(job, 10.0, EncodePhase::EncodingVideo).await;
        let video_output = temp_dir.join("video.mkv");

        // Set up progress channel for av1an
        let (progress_tx, mut progress_rx) = mpsc::channel(100);

        // Spawn av1an with progress tracking
        let input = job.input_path.clone();
        let output = video_output.clone();
        let profile_clone = profile.clone();

        let encode_handle = tokio::spawn(async move {
            av1an::encode(&input, &output, &profile_clone, Some(progress_tx)).await
        });

        // Forward progress updates
        let job_id = job.id.clone();
        let self_progress_tx = self.progress_tx.clone();
        tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                if let Some(tx) = &self_progress_tx {
                    let _ = tx.send(WorkerProgress {
                        job_id: job_id.clone(),
                        percent: 10.0 + (progress.percent * 0.7), // 10% to 80%
                        phase: EncodePhase::EncodingVideo,
                    }).await;
                }
            }
        });

        // Wait for encode to complete
        encode_handle.await.map_err(|e| EncoderError::SpawnFailed(e.to_string()))??;

        // Phase 5: Handle subtitle burn-in if needed
        let final_video = if let Some(sub) = burn_in_sub {
            let burned_output = temp_dir.join("video_burned.mkv");
            ffmpeg::burn_subtitles(&video_output, &sub.path, &burned_output, true).await?;
            burned_output
        } else {
            video_output
        };

        // Phase 6: Process audio
        self.send_progress(job, 85.0, EncodePhase::ProcessingAudio).await;
        let audio_output = temp_dir.join("audio.mka");
        ffmpeg::process_audio(&job.input_path, &audio_output, &audio_decisions).await?;

        // Phase 7: Mux final output
        self.send_progress(job, 95.0, EncodePhase::Muxing).await;

        // Ensure output directory exists
        if let Some(parent) = job.output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;
        }

        mkvmerge::mux(&final_video, &audio_output, &extracted_subs, &job.output_path).await?;

        // Phase 8: Verify output
        self.send_progress(job, 99.0, EncodePhase::Verifying).await;
        let output_probe = probe::probe(&job.output_path)
            .map_err(|e| EncoderError::VerificationFailed(e.to_string()))?;

        // Verify output has video
        if output_probe.video_streams.is_empty() {
            return Err(EncoderError::VerificationFailed("No video stream in output".to_string()));
        }

        // Build result metadata
        let encode_duration = start_time.elapsed().as_secs_f64();
        let metadata = EncodeResultMetadata {
            input_size: probe_result.info.size,
            output_size: output_probe.info.size,
            encode_duration_secs: encode_duration,
            vmaf_score: None, // TODO: Could be parsed from av1an output
            video_duration_secs: probe_result.info.duration,
            encoding_speed: probe_result.info.duration / encode_duration,
        };

        job.complete(metadata);
        self.send_progress(job, 100.0, EncodePhase::Verifying).await;

        Ok(())
    }

    /// Handles a job failure.
    async fn handle_failure(&mut self, job: EncodeJob, error: String) -> Result<()> {
        let mut handler = DeadLetterHandler::new(&mut self.queue, self.max_attempts);

        match handler.handle_failure(job, error).await {
            Ok(FailureAction::Retrying { attempt, max_attempts }) => {
                info!(attempt, max_attempts, "Job will be retried");
            }
            Ok(FailureAction::DeadLettered { reason }) => {
                warn!(reason, "Job moved to dead letter queue");
                // TODO: Send notification
            }
            Err(e) => {
                error!(error = %e, "Failed to handle job failure");
            }
        }

        Ok(())
    }

    /// Sends a progress update.
    async fn send_progress(&self, job: &EncodeJob, percent: f32, phase: EncodePhase) {
        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(WorkerProgress {
                job_id: job.id.clone(),
                percent,
                phase,
            }).await;
        }
    }
}
