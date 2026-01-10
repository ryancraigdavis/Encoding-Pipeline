//! Codec availability validation.

use crate::config::model::{AppConfig, Encoder};

use super::{SystemCapabilities, ValidationIssue, ValidationResult};

/// Validates that all requested codecs are available on the system.
pub fn validate(config: &AppConfig, capabilities: &SystemCapabilities) -> ValidationResult {
    let mut result = ValidationResult::new();

    for (i, profile) in config.profiles.iter().enumerate() {
        let prefix = format!("profiles[{}]", i);

        // Check video encoder availability
        let encoder_name = profile.encoder.to_string();
        if !capabilities.av1an_encoders.contains(&encoder_name) {
            result.add(
                ValidationIssue::error(
                    format!("{}.encoder", prefix),
                    format!("Video encoder '{}' is not available", encoder_name),
                )
                .with_suggestion(format!(
                    "Available encoders: {}",
                    format_available(&capabilities.av1an_encoders)
                )),
            );
        }

        // Check audio codecs in transcode settings
        for (j, rule) in profile.audio.rules.iter().enumerate() {
            if let Some(transcode) = &rule.transcode {
                let codec = normalize_codec_name(&transcode.codec);
                if !capabilities.available_encoders.contains(&codec) {
                    result.add(
                        ValidationIssue::error(
                            format!("{}.audio.rules[{}].transcode.codec", prefix, j),
                            format!("Audio codec '{}' is not available", transcode.codec),
                        )
                        .with_suggestion(suggest_audio_codec(&transcode.codec, capabilities)),
                    );
                }
            }

            if let Some(downmix) = &rule.downmix {
                let codec = normalize_codec_name(&downmix.codec);
                if !capabilities.available_encoders.contains(&codec) {
                    result.add(
                        ValidationIssue::error(
                            format!("{}.audio.rules[{}].downmix.codec", prefix, j),
                            format!("Audio codec '{}' is not available", downmix.codec),
                        )
                        .with_suggestion(suggest_audio_codec(&downmix.codec, capabilities)),
                    );
                }
            }
        }
    }

    result
}

/// Normalizes codec names to FFmpeg encoder names.
fn normalize_codec_name(codec: &str) -> String {
    match codec.to_lowercase().as_str() {
        "aac" => "aac".to_string(),
        "ac3" => "ac3".to_string(),
        "eac3" | "e-ac3" => "eac3".to_string(),
        "opus" => "libopus".to_string(),
        "mp3" => "libmp3lame".to_string(),
        "flac" => "flac".to_string(),
        "vorbis" => "libvorbis".to_string(),
        other => other.to_string(),
    }
}

/// Suggests an alternative audio codec if the requested one is not available.
fn suggest_audio_codec(requested: &str, capabilities: &SystemCapabilities) -> String {
    let common_codecs = ["aac", "libopus", "ac3", "flac", "libmp3lame"];

    let available: Vec<&str> = common_codecs
        .iter()
        .filter(|c| capabilities.available_encoders.contains(&c.to_string()))
        .copied()
        .collect();

    if available.is_empty() {
        "No common audio encoders available".to_string()
    } else {
        format!("Available alternatives: {}", available.join(", "))
    }
}

/// Formats a set of available items for display.
fn format_available(items: &std::collections::HashSet<String>) -> String {
    let mut sorted: Vec<_> = items.iter().collect();
    sorted.sort();
    sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
}
