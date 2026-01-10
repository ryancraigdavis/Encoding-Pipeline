//! Semantic validation for configuration values.

use std::collections::HashSet;

use crate::config::model::{AppConfig, AudioAction, DownmixMode};

use super::{ValidationIssue, ValidationResult};

/// Valid ISO 639-2 language codes (common subset).
const VALID_LANGUAGE_CODES: &[&str] = &[
    "eng", "jpn", "deu", "fra", "spa", "ita", "por", "rus", "zho", "kor", "ara", "hin", "tha",
    "vie", "ind", "msa", "pol", "nld", "swe", "nor", "dan", "fin", "ces", "hun", "ron", "tur",
    "ell", "heb", "ukr", "bul", "hrv", "srp", "slv", "slk", "lit", "lav", "est", "und", "mul",
];

/// Valid audio codec names.
const VALID_AUDIO_CODECS: &[&str] = &[
    "aac", "ac3", "eac3", "dts", "truehd", "flac", "opus", "libopus", "mp3", "libmp3lame",
    "vorbis", "libvorbis", "pcm_s16le", "pcm_s24le", "pcm_s32le",
];

/// Validates semantic correctness of configuration values.
pub fn validate(config: &AppConfig) -> ValidationResult {
    let mut result = ValidationResult::new();

    // Validate global settings
    validate_global(&config.global, &mut result);

    // Check for duplicate profile names
    let mut seen_names = HashSet::new();
    let mut seen_paths = HashSet::new();

    for (i, profile) in config.profiles.iter().enumerate() {
        let prefix = format!("profiles[{}]", i);

        // Check duplicate names
        if !seen_names.insert(&profile.name) {
            result.add(ValidationIssue::error(
                format!("{}.name", prefix),
                format!("Duplicate profile name: '{}'", profile.name),
            ));
        }

        // Check duplicate input paths
        let input_str = profile.input_path.to_string_lossy().to_string();
        if !seen_paths.insert(input_str.clone()) {
            result.add(ValidationIssue::error(
                format!("{}.input_path", prefix),
                format!("Duplicate input path: '{}'", input_str),
            ));
        }

        // Validate VMAF target range
        if profile.vmaf_target < 0.0 || profile.vmaf_target > 100.0 {
            result.add(
                ValidationIssue::error(
                    format!("{}.vmaf_target", prefix),
                    format!(
                        "VMAF target {} is out of range",
                        profile.vmaf_target
                    ),
                )
                .with_suggestion("VMAF target must be between 0 and 100"),
            );
        }

        // Validate workers count
        if profile.workers == 0 {
            result.add(ValidationIssue::error(
                format!("{}.workers", prefix),
                "Workers must be at least 1",
            ));
        }

        // Validate audio rules
        validate_audio_rules(&profile.audio.rules, &prefix, &mut result);

        // Validate language priority codes
        for (j, lang) in profile.audio.language_priority.iter().enumerate() {
            if !VALID_LANGUAGE_CODES.contains(&lang.as_str()) {
                result.add(
                    ValidationIssue::warning(
                        format!("{}.audio.language_priority[{}]", prefix, j),
                        format!("Unknown language code: '{}'", lang),
                    )
                    .with_suggestion(format!(
                        "Common codes: eng, jpn, deu, fra, spa. Did you mean '{}'?",
                        find_similar_language(lang)
                    )),
                );
            }
        }

        // Validate subtitle track languages
        for (j, track) in profile.subtitles.tracks.iter().enumerate() {
            if !VALID_LANGUAGE_CODES.contains(&track.language.as_str()) {
                result.add(
                    ValidationIssue::warning(
                        format!("{}.subtitles.tracks[{}].language", prefix, j),
                        format!("Unknown language code: '{}'", track.language),
                    )
                    .with_suggestion(format!(
                        "Common codes: eng, jpn, deu, fra, spa. Did you mean '{}'?",
                        find_similar_language(&track.language)
                    )),
                );
            }
        }
    }

    result
}

/// Validates global configuration settings.
fn validate_global(global: &crate::config::model::GlobalConfig, result: &mut ValidationResult) {
    // Validate log level
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&global.log_level.as_str()) {
        result.add(
            ValidationIssue::error("global.log_level", format!("Invalid log level: '{}'", global.log_level))
                .with_suggestion(format!("Valid levels: {}", valid_levels.join(", "))),
        );
    }

    // Validate Redis port
    if global.redis.port == 0 {
        result.add(ValidationIssue::error(
            "global.redis.port",
            "Redis port cannot be 0",
        ));
    }

    // Validate stability check settings
    if global.stability_check.duration_seconds == 0 {
        result.add(ValidationIssue::error(
            "global.stability_check.duration_seconds",
            "Stability duration must be at least 1 second",
        ));
    }

    if global.stability_check.poll_interval_seconds == 0 {
        result.add(ValidationIssue::error(
            "global.stability_check.poll_interval_seconds",
            "Poll interval must be at least 1 second",
        ));
    }

    // Validate Prometheus port
    if global.prometheus.enabled && global.prometheus.port == 0 {
        result.add(ValidationIssue::error(
            "global.prometheus.port",
            "Prometheus port cannot be 0 when enabled",
        ));
    }
}

/// Validates audio processing rules.
fn validate_audio_rules(
    rules: &[crate::config::model::AudioRule],
    prefix: &str,
    result: &mut ValidationResult,
) {
    for (j, rule) in rules.iter().enumerate() {
        let rule_prefix = format!("{}.audio.rules[{}]", prefix, j);

        // Validate language codes in match criteria
        if let Some(lang) = &rule.match_criteria.language {
            if !VALID_LANGUAGE_CODES.contains(&lang.as_str()) {
                result.add(
                    ValidationIssue::warning(
                        format!("{}.match.language", rule_prefix),
                        format!("Unknown language code: '{}'", lang),
                    )
                    .with_suggestion(format!("Did you mean '{}'?", find_similar_language(lang))),
                );
            }
        }

        if let Some(languages) = &rule.match_criteria.languages {
            for (k, lang) in languages.iter().enumerate() {
                if !VALID_LANGUAGE_CODES.contains(&lang.as_str()) {
                    result.add(
                        ValidationIssue::warning(
                            format!("{}.match.languages[{}]", rule_prefix, k),
                            format!("Unknown language code: '{}'", lang),
                        )
                        .with_suggestion(format!("Did you mean '{}'?", find_similar_language(lang))),
                    );
                }
            }
        }

        // Validate passthrough codecs
        for (k, codec) in rule.passthrough_codecs.iter().enumerate() {
            if !VALID_AUDIO_CODECS.contains(&codec.as_str()) {
                result.add(
                    ValidationIssue::warning(
                        format!("{}.passthrough_codecs[{}]", rule_prefix, k),
                        format!("Unknown audio codec: '{}'", codec),
                    )
                    .with_suggestion(format!(
                        "Valid codecs: {}",
                        VALID_AUDIO_CODECS[..5].join(", ")
                    )),
                );
            }
        }

        // Validate transcode settings when action requires it
        match rule.action {
            AudioAction::Transcode | AudioAction::PassthroughOrTranscode => {
                if rule.transcode.is_none() {
                    result.add(ValidationIssue::error(
                        format!("{}.transcode", rule_prefix),
                        "Transcode settings required for this action",
                    ));
                } else if let Some(transcode) = &rule.transcode {
                    validate_bitrate(&transcode.bitrate, &format!("{}.transcode.bitrate", rule_prefix), result);
                    if let Some(lossless_bitrate) = &transcode.lossless_bitrate {
                        validate_bitrate(lossless_bitrate, &format!("{}.transcode.lossless_bitrate", rule_prefix), result);
                    }
                }
            }
            _ => {}
        }

        // Validate downmix settings
        if let Some(downmix) = &rule.downmix {
            if !matches!(downmix.mode, DownmixMode::None) {
                validate_bitrate(&downmix.bitrate, &format!("{}.downmix.bitrate", rule_prefix), result);
            }
        }
    }
}

/// Validates a bitrate string format (e.g., "128k", "1.5m").
fn validate_bitrate(bitrate: &str, path: &str, result: &mut ValidationResult) {
    let valid = bitrate
        .strip_suffix('k')
        .or_else(|| bitrate.strip_suffix('K'))
        .or_else(|| bitrate.strip_suffix('m'))
        .or_else(|| bitrate.strip_suffix('M'))
        .map(|num| num.parse::<f32>().is_ok())
        .unwrap_or(false);

    if !valid {
        result.add(
            ValidationIssue::error(path, format!("Invalid bitrate format: '{}'", bitrate))
                .with_suggestion("Expected format: '128k', '256k', '640k', or '1.5m'"),
        );
    }
}

/// Finds the most similar language code using Levenshtein distance.
fn find_similar_language(input: &str) -> &'static str {
    let input_lower = input.to_lowercase();

    // Check for common mistakes
    let common_mappings = [
        ("english", "eng"),
        ("japanese", "jpn"),
        ("german", "deu"),
        ("french", "fra"),
        ("spanish", "spa"),
        ("en", "eng"),
        ("jp", "jpn"),
        ("de", "deu"),
        ("fr", "fra"),
        ("es", "spa"),
    ];

    for (mistake, correct) in common_mappings {
        if input_lower == mistake {
            return correct;
        }
    }

    // Fall back to Levenshtein distance
    VALID_LANGUAGE_CODES
        .iter()
        .min_by_key(|code| strsim::levenshtein(&input_lower, code))
        .copied()
        .unwrap_or("eng")
}
