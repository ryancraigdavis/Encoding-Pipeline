//! FFmpeg subprocess wrapper for audio processing.

use std::path::Path;
use std::process::Stdio;

use anyhow::Result;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::EncoderError;
use crate::media::audio::{AudioDecision, AudioTrackAction};
use crate::media::subtitle::{SubtitleDecision, SubtitleTrackAction};

/// Processes audio tracks according to the given decisions.
pub async fn process_audio(
    input: &Path,
    output: &Path,
    decisions: &[AudioDecision],
) -> Result<(), EncoderError> {
    let mut cmd = Command::new("ffmpeg");

    cmd.arg("-y"); // Overwrite output
    cmd.arg("-i").arg(input);

    // Build map and codec arguments
    let mut audio_index = 0;

    for decision in decisions {
        match &decision.action {
            AudioTrackAction::Exclude => continue,

            AudioTrackAction::Passthrough => {
                cmd.arg("-map").arg(format!("0:{}", decision.stream.index));
                cmd.arg(format!("-c:a:{}", audio_index)).arg("copy");
                audio_index += 1;
            }

            AudioTrackAction::Transcode { codec, bitrate } => {
                cmd.arg("-map").arg(format!("0:{}", decision.stream.index));
                cmd.arg(format!("-c:a:{}", audio_index)).arg(normalize_codec(codec));
                cmd.arg(format!("-b:a:{}", audio_index)).arg(bitrate);
                audio_index += 1;
            }

            AudioTrackAction::PassthroughWithDownmix { downmix_codec, downmix_bitrate } => {
                // Original track
                cmd.arg("-map").arg(format!("0:{}", decision.stream.index));
                cmd.arg(format!("-c:a:{}", audio_index)).arg("copy");
                audio_index += 1;

                // Downmixed stereo track
                cmd.arg("-map").arg(format!("0:{}", decision.stream.index));
                cmd.arg(format!("-c:a:{}", audio_index)).arg(normalize_codec(downmix_codec));
                cmd.arg(format!("-ac:{}", audio_index)).arg("2");
                cmd.arg(format!("-b:a:{}", audio_index)).arg(downmix_bitrate);
                audio_index += 1;
            }

            AudioTrackAction::TranscodeWithDownmix { codec, bitrate, downmix_codec, downmix_bitrate } => {
                // Transcoded track
                cmd.arg("-map").arg(format!("0:{}", decision.stream.index));
                cmd.arg(format!("-c:a:{}", audio_index)).arg(normalize_codec(codec));
                cmd.arg(format!("-b:a:{}", audio_index)).arg(bitrate);
                audio_index += 1;

                // Downmixed stereo track
                cmd.arg("-map").arg(format!("0:{}", decision.stream.index));
                cmd.arg(format!("-c:a:{}", audio_index)).arg(normalize_codec(downmix_codec));
                cmd.arg(format!("-ac:{}", audio_index)).arg("2");
                cmd.arg(format!("-b:a:{}", audio_index)).arg(downmix_bitrate);
                audio_index += 1;
            }
        }
    }

    // No video, just audio
    cmd.arg("-vn");

    cmd.arg(output);

    debug!(cmd = ?cmd, "Running FFmpeg for audio");

    let output_result = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        return Err(EncoderError::FfmpegFailed {
            code: output_result.status.code().unwrap_or(-1),
            stderr: stderr.to_string(),
        });
    }

    info!("Audio processing completed");
    Ok(())
}

/// Extracts subtitles to separate files.
pub async fn extract_subtitles(
    input: &Path,
    output_dir: &Path,
    decisions: &[SubtitleDecision],
) -> Result<Vec<ExtractedSubtitle>, EncoderError> {
    let mut extracted = Vec::new();

    for decision in decisions {
        if matches!(decision.action, SubtitleTrackAction::Exclude) {
            continue;
        }

        let stream = &decision.stream;
        let ext = if stream.is_image_based { "sup" } else { "srt" };
        let output_file = output_dir.join(format!(
            "sub_{}_{}.{}",
            stream.index,
            stream.language.as_deref().unwrap_or("und"),
            ext
        ));

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y");
        cmd.arg("-i").arg(input);
        cmd.arg("-map").arg(format!("0:{}", stream.index));
        cmd.arg("-c:s").arg("copy");
        cmd.arg(&output_file);

        let output_result = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

        if output_result.status.success() {
            extracted.push(ExtractedSubtitle {
                path: output_file,
                stream_index: stream.index,
                language: stream.language.clone(),
                is_forced: stream.is_forced,
                is_default: stream.is_default,
                should_burn_in: matches!(decision.action, SubtitleTrackAction::BurnIn),
            });
        } else {
            debug!(
                stream_index = stream.index,
                "Failed to extract subtitle stream"
            );
        }
    }

    Ok(extracted)
}

/// An extracted subtitle file.
#[derive(Debug)]
pub struct ExtractedSubtitle {
    /// Path to the extracted subtitle file.
    pub path: std::path::PathBuf,
    /// Original stream index.
    pub stream_index: usize,
    /// Language code.
    pub language: Option<String>,
    /// Whether this is a forced track.
    pub is_forced: bool,
    /// Whether this is the default track.
    pub is_default: bool,
    /// Whether this should be burned into the video.
    pub should_burn_in: bool,
}

/// Burns subtitles into a video.
pub async fn burn_subtitles(
    input: &Path,
    subtitle: &Path,
    output: &Path,
    is_image_based: bool,
) -> Result<(), EncoderError> {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");
    cmd.arg("-i").arg(input);

    if is_image_based {
        // For PGS/image subtitles, use overlay filter
        cmd.arg("-i").arg(subtitle);
        cmd.arg("-filter_complex").arg("[0:v][1:s]overlay[v]");
        cmd.arg("-map").arg("[v]");
        cmd.arg("-map").arg("0:a");
        cmd.arg("-c:a").arg("copy");
    } else {
        // For text subtitles, use subtitles filter
        let subtitle_path = subtitle.to_string_lossy().replace('\\', "/").replace(':', "\\:");
        cmd.arg("-vf").arg(format!("subtitles='{}'", subtitle_path));
        cmd.arg("-map").arg("0:a");
        cmd.arg("-c:a").arg("copy");
    }

    cmd.arg(output);

    let output_result = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| EncoderError::SpawnFailed(e.to_string()))?;

    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        return Err(EncoderError::FfmpegFailed {
            code: output_result.status.code().unwrap_or(-1),
            stderr: stderr.to_string(),
        });
    }

    info!("Subtitle burn-in completed");
    Ok(())
}

/// Normalizes a codec name to FFmpeg encoder name.
fn normalize_codec(codec: &str) -> &str {
    match codec.to_lowercase().as_str() {
        "opus" => "libopus",
        "mp3" => "libmp3lame",
        "vorbis" => "libvorbis",
        _ => codec,
    }
}
