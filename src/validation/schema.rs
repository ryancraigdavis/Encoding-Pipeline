//! Schema validation for configuration structure.

use crate::config::model::AppConfig;
use super::{ValidationIssue, ValidationResult};

/// Validates the configuration schema (required fields, structure).
pub fn validate(config: &AppConfig) -> ValidationResult {
    let mut result = ValidationResult::new();

    // Validate profiles exist
    if config.profiles.is_empty() {
        result.add(ValidationIssue::error(
            "profiles",
            "At least one profile is required",
        ));
    }

    // Validate each profile
    for (i, profile) in config.profiles.iter().enumerate() {
        let prefix = format!("profiles[{}]", i);

        // Profile name must not be empty
        if profile.name.trim().is_empty() {
            result.add(ValidationIssue::error(
                format!("{}.name", prefix),
                "Profile name cannot be empty",
            ));
        }

        // Input path must be set
        if profile.input_path.as_os_str().is_empty() {
            result.add(ValidationIssue::error(
                format!("{}.input_path", prefix),
                "Input path is required",
            ));
        }

        // Output path must be set
        if profile.output_path.as_os_str().is_empty() {
            result.add(ValidationIssue::error(
                format!("{}.output_path", prefix),
                "Output path is required",
            ));
        }

        // Validate audio rules
        if profile.audio.rules.is_empty() {
            result.add(
                ValidationIssue::warning(
                    format!("{}.audio.rules", prefix),
                    "No audio rules defined, all tracks will use fallback behavior",
                )
                .with_suggestion("Add at least one audio rule or set fallback to 'passthrough'"),
            );
        }

        // Validate subtitle tracks
        if profile.subtitles.tracks.is_empty() {
            result.add(
                ValidationIssue::warning(
                    format!("{}.subtitles.tracks", prefix),
                    "No subtitle tracks defined, all tracks will use fallback behavior",
                )
                .with_suggestion("Add at least one subtitle language or set fallback to 'include'"),
            );
        }
    }

    result
}
