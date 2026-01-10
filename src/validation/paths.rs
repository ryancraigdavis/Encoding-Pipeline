//! Path validation for configuration directories.

use std::collections::HashSet;
use std::path::Path;

use crate::config::model::AppConfig;

use super::{ValidationIssue, ValidationResult};

/// Validates that all configured paths exist and are accessible.
pub fn validate(config: &AppConfig) -> ValidationResult {
    let mut result = ValidationResult::new();

    // Validate temp directory
    validate_directory_writable(&config.global.temp_dir, "global.temp_dir", &mut result);

    // Track paths to check for overlaps
    let mut input_paths: Vec<(&str, &Path)> = Vec::new();

    for (i, profile) in config.profiles.iter().enumerate() {
        let prefix = format!("profiles[{}]", i);

        // Validate input path exists and is readable
        validate_directory_readable(&profile.input_path, &format!("{}.input_path", prefix), &mut result);

        // Validate output path exists and is writable
        validate_directory_writable(&profile.output_path, &format!("{}.output_path", prefix), &mut result);

        // Input and output paths should not be the same
        if profile.input_path == profile.output_path {
            result.add(ValidationIssue::error(
                format!("{}.output_path", prefix),
                "Input and output paths cannot be the same",
            ));
        }

        // Collect input paths for overlap check
        input_paths.push((&profile.name, &profile.input_path));
    }

    // Check for overlapping input paths
    validate_no_path_overlaps(&input_paths, &mut result);

    result
}

/// Validates that a directory exists and is readable.
fn validate_directory_readable(path: &Path, config_path: &str, result: &mut ValidationResult) {
    if !path.exists() {
        result.add(
            ValidationIssue::error(
                config_path,
                format!("Directory does not exist: '{}'", path.display()),
            )
            .with_suggestion("Create the directory or update the path"),
        );
        return;
    }

    if !path.is_dir() {
        result.add(ValidationIssue::error(
            config_path,
            format!("Path is not a directory: '{}'", path.display()),
        ));
        return;
    }

    // Try to read the directory to check permissions
    if std::fs::read_dir(path).is_err() {
        result.add(
            ValidationIssue::error(
                config_path,
                format!("Directory is not readable: '{}'", path.display()),
            )
            .with_suggestion("Check directory permissions"),
        );
    }
}

/// Validates that a directory exists and is writable.
fn validate_directory_writable(path: &Path, config_path: &str, result: &mut ValidationResult) {
    if !path.exists() {
        // Try to create it
        if let Err(e) = std::fs::create_dir_all(path) {
            result.add(
                ValidationIssue::error(
                    config_path,
                    format!("Cannot create directory '{}': {}", path.display(), e),
                )
                .with_suggestion("Check parent directory permissions"),
            );
        }
        return;
    }

    if !path.is_dir() {
        result.add(ValidationIssue::error(
            config_path,
            format!("Path is not a directory: '{}'", path.display()),
        ));
        return;
    }

    // Try to write a test file to check permissions
    let test_file = path.join(".write_test");
    match std::fs::write(&test_file, "test") {
        Ok(()) => {
            let _ = std::fs::remove_file(&test_file);
        }
        Err(e) => {
            result.add(
                ValidationIssue::error(
                    config_path,
                    format!("Directory is not writable '{}': {}", path.display(), e),
                )
                .with_suggestion("Check directory permissions"),
            );
        }
    }
}

/// Validates that input paths do not overlap (one being a subdirectory of another).
fn validate_no_path_overlaps(paths: &[(&str, &Path)], result: &mut ValidationResult) {
    for (i, (name_a, path_a)) in paths.iter().enumerate() {
        for (name_b, path_b) in paths.iter().skip(i + 1) {
            if paths_overlap(path_a, path_b) {
                result.add(
                    ValidationIssue::error(
                        format!("profiles[{}].input_path", i),
                        format!(
                            "Input paths overlap: '{}' ({}) and '{}' ({})",
                            path_a.display(),
                            name_a,
                            path_b.display(),
                            name_b
                        ),
                    )
                    .with_suggestion("Each profile should watch a distinct directory tree"),
                );
            }
        }
    }
}

/// Checks if two paths overlap (one is a parent/child of the other).
fn paths_overlap(path_a: &Path, path_b: &Path) -> bool {
    // Canonicalize paths for accurate comparison
    let canon_a = std::fs::canonicalize(path_a).unwrap_or_else(|_| path_a.to_path_buf());
    let canon_b = std::fs::canonicalize(path_b).unwrap_or_else(|_| path_b.to_path_buf());

    canon_a.starts_with(&canon_b) || canon_b.starts_with(&canon_a)
}
