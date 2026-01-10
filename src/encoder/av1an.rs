//! av1an subprocess wrapper for video encoding.

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::config::model::{Encoder, Profile};
use crate::error::EncoderError;

/// Progress update from av1an.
#[derive(Debug, Clone)]
pub struct EncodeProgress {
    /// Percentage complete (0-100).
    pub percent: f32,
    /// Current encoding speed (e.g., "2.5x").
    pub speed: Option<String>,
    /// Estimated time remaining.
    pub eta: Option<String>,
    /// Current frame being encoded.
    pub frame: Option<u64>,
    /// Total frames to encode.
    pub total_frames: Option<u64>,
}

/// Encodes video using av1an with VMAF targeting.
pub async fn encode(
    input: &Path,
    output: &Path,
    profile: &Profile,
    progress_tx: Option<mpsc::Sender<EncodeProgress>>,
) -> Result<(), EncoderError> {
    let temp_dir = std::env::temp_dir().join(format!(
        "av1an_{}",
        uuid::Uuid::new_v4()
    ));

    std::fs::create_dir_all(&temp_dir).map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

    // Build av1an command
    let mut cmd = Command::new("av1an");

    cmd.arg("-i").arg(input);
    cmd.arg("-o").arg(output);
    cmd.arg("--temp").arg(&temp_dir);

    // Set encoder
    let encoder_name = match profile.encoder {
        Encoder::X265 => "x265",
        Encoder::X264 => "x264",
        Encoder::SvtAv1 => "svt-av1",
        Encoder::Aomenc => "aom",
        Encoder::Rav1e => "rav1e",
    };
    cmd.arg("--encoder").arg(encoder_name);

    // Set VMAF target
    cmd.arg("--target-quality").arg(profile.vmaf_target.to_string());
    cmd.arg("--target-metric").arg("vmaf");

    // Set workers
    cmd.arg("-w").arg(profile.workers.to_string());

    // Set encoder params if provided
    if !profile.encoder_params.is_empty() {
        cmd.arg("-v").arg(&profile.encoder_params);
    }

    // Use lsmash for chunking (best for MKV)
    cmd.arg("--chunk-method").arg("lsmash");

    // Enable resume in case of interruption
    cmd.arg("--resume");

    // Configure output
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    info!(
        input = ?input,
        output = ?output,
        encoder = encoder_name,
        vmaf_target = profile.vmaf_target,
        "Starting av1an encode"
    );

    let mut child = cmd.spawn().map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

    // Read stderr for progress
    if let Some(stderr) = child.stderr.take() {
        let progress_tx = progress_tx.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                debug!(line = %line, "av1an output");

                if let Some(progress) = parse_progress(&line) {
                    if let Some(tx) = &progress_tx {
                        let _ = tx.send(progress).await;
                    }
                }
            }
        });
    }

    let status = child
        .wait()
        .await
        .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

    // Clean up temp directory
    if let Err(e) = std::fs::remove_dir_all(&temp_dir) {
        debug!(error = %e, "Failed to clean up temp directory");
    }

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(EncoderError::Av1anFailed {
            code,
            stderr: "Encoding failed".to_string(),
        });
    }

    info!("av1an encode completed successfully");
    Ok(())
}

/// Parses progress information from av1an output.
fn parse_progress(line: &str) -> Option<EncodeProgress> {
    // av1an progress format varies, but typically includes percentage
    // Example: "Encoding: 50.0% - speed: 2.5x - ETA: 1:30:00"

    let line_lower = line.to_lowercase();

    if !line_lower.contains("encoding") && !line_lower.contains("%") {
        return None;
    }

    // Try to extract percentage
    let percent = extract_percentage(line)?;

    // Try to extract speed
    let speed = extract_pattern(line, "speed:", "x")
        .or_else(|| extract_pattern(line, "fps:", " "));

    // Try to extract ETA
    let eta = extract_pattern(line, "eta:", " ")
        .or_else(|| extract_pattern(line, "remaining:", " "));

    // Try to extract frame info
    let frame = extract_number_after(line, "frame");
    let total_frames = extract_number_after(line, "of");

    Some(EncodeProgress {
        percent,
        speed,
        eta,
        frame,
        total_frames,
    })
}

/// Extracts a percentage value from a line.
fn extract_percentage(line: &str) -> Option<f32> {
    for part in line.split_whitespace() {
        if let Some(num_str) = part.strip_suffix('%') {
            if let Ok(num) = num_str.parse::<f32>() {
                return Some(num);
            }
        }
    }
    None
}

/// Extracts a pattern like "prefix: value suffix" from a line.
fn extract_pattern(line: &str, prefix: &str, suffix: &str) -> Option<String> {
    let line_lower = line.to_lowercase();
    let start = line_lower.find(&prefix.to_lowercase())?;
    let after_prefix = &line[start + prefix.len()..];
    let trimmed = after_prefix.trim_start();

    if suffix.is_empty() {
        let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        Some(trimmed[..end].to_string())
    } else {
        let end = trimmed.find(suffix)?;
        Some(format!("{}{}", trimmed[..end].trim(), suffix))
    }
}

/// Extracts a number after a keyword.
fn extract_number_after(line: &str, keyword: &str) -> Option<u64> {
    let line_lower = line.to_lowercase();
    let start = line_lower.find(&keyword.to_lowercase())?;
    let after = &line[start + keyword.len()..];

    for part in after.split_whitespace() {
        if let Ok(num) = part.parse::<u64>() {
            return Some(num);
        }
    }
    None
}
