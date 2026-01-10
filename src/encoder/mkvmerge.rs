//! mkvmerge subprocess wrapper for MKV muxing.

use std::path::Path;
use std::process::Stdio;

use anyhow::Result;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::EncoderError;

use super::ffmpeg::ExtractedSubtitle;

/// Muxes video, audio, and subtitles into an MKV file.
pub async fn mux(
    video: &Path,
    audio: &Path,
    subtitles: &[ExtractedSubtitle],
    output: &Path,
) -> Result<(), EncoderError> {
    let mut cmd = Command::new("mkvmerge");

    cmd.arg("-o").arg(output);

    // Add video (no audio from video file)
    cmd.arg("--no-audio").arg("--no-subtitles").arg(video);

    // Add audio (no video from audio file)
    cmd.arg("--no-video").arg("--no-subtitles").arg(audio);

    // Add subtitles
    for sub in subtitles.iter().filter(|s| !s.should_burn_in) {
        // Set language
        if let Some(lang) = &sub.language {
            cmd.arg("--language").arg(format!("0:{}", lang));
        }

        // Set forced flag
        if sub.is_forced {
            cmd.arg("--forced-display-flag").arg("0:yes");
        }

        // Set default flag
        if sub.is_default {
            cmd.arg("--default-track-flag").arg("0:yes");
        }

        cmd.arg(&sub.path);
    }

    debug!(cmd = ?cmd, "Running mkvmerge");

    let output_result = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

    // mkvmerge returns 0 for success, 1 for warnings, 2 for errors
    if output_result.status.code().unwrap_or(2) >= 2 {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        return Err(EncoderError::MkvmergeFailed {
            code: output_result.status.code().unwrap_or(-1),
            stderr: stderr.to_string(),
        });
    }

    info!("MKV muxing completed");
    Ok(())
}

/// Remuxes a file to MKV without re-encoding.
pub async fn remux(input: &Path, output: &Path) -> Result<(), EncoderError> {
    let mut cmd = Command::new("mkvmerge");

    cmd.arg("-o").arg(output);
    cmd.arg(input);

    let output_result = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

    if output_result.status.code().unwrap_or(2) >= 2 {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        return Err(EncoderError::MkvmergeFailed {
            code: output_result.status.code().unwrap_or(-1),
            stderr: stderr.to_string(),
        });
    }

    Ok(())
}

/// Sets track properties on an existing MKV file.
pub async fn set_track_properties(
    file: &Path,
    track_index: usize,
    language: Option<&str>,
    name: Option<&str>,
    is_default: bool,
    is_forced: bool,
) -> Result<(), EncoderError> {
    let mut cmd = Command::new("mkvpropedit");

    cmd.arg(file);
    cmd.arg("--edit").arg(format!("track:{}", track_index + 1));

    if let Some(lang) = language {
        cmd.arg("--set").arg(format!("language={}", lang));
    }

    if let Some(n) = name {
        cmd.arg("--set").arg(format!("name={}", n));
    }

    cmd.arg("--set").arg(format!("flag-default={}", if is_default { "1" } else { "0" }));
    cmd.arg("--set").arg(format!("flag-forced={}", if is_forced { "1" } else { "0" }));

    let output_result = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        return Err(EncoderError::MkvmergeFailed {
            code: output_result.status.code().unwrap_or(-1),
            stderr: stderr.to_string(),
        });
    }

    Ok(())
}
