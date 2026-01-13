//! Configuration file loading and parsing.

use std::path::Path;

use anyhow::{Context, Result};

use super::model::AppConfig;
use crate::error::ConfigError;
use crate::validation::{validate_config, SystemCapabilities};

/// Loads the configuration file from disk and parses it.
pub fn load_from_path(path: &Path) -> Result<AppConfig, ConfigError> {
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError::ReadFailed {
        path: path.to_path_buf(),
        source: e,
    })?;

    let config: AppConfig =
        serde_yaml::from_str(&content).map_err(|e| ConfigError::ParseFailed {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

    Ok(config)
}

/// Loads and fully validates the configuration file.
pub fn load_and_validate(path: &Path, capabilities: &SystemCapabilities) -> Result<AppConfig> {
    let config = load_from_path(path).context("Failed to load configuration")?;

    let result = validate_config(&config, capabilities);

    // Log warnings
    for issue in result.warnings() {
        tracing::warn!(
            path = %issue.path,
            message = %issue.message,
            suggestion = ?issue.suggestion,
            "Config validation warning"
        );
    }

    // Check for errors
    let errors: Vec<_> = result.errors().collect();
    if !errors.is_empty() {
        let report = format_validation_errors(&errors);
        tracing::error!("{}", report);
        anyhow::bail!(ConfigError::ValidationFailed {
            error_count: errors.len()
        });
    }

    Ok(config)
}

/// Formats validation errors into a human-readable report.
fn format_validation_errors(errors: &[&crate::validation::ValidationIssue]) -> String {
    let mut report = String::from("\nConfig Validation Failed\n");
    report.push_str("========================\n\n");

    for error in errors {
        report.push_str(&format!("ERROR {}\n", error.path));
        report.push_str(&format!("  └─ {}\n", error.message));
        if let Some(suggestion) = &error.suggestion {
            report.push_str(&format!("     {}\n", suggestion));
        }
        report.push('\n');
    }

    report.push_str(&format!(
        "---\n{} error(s)\nConfig rejected. Current config unchanged.\n",
        errors.len()
    ));

    report
}
