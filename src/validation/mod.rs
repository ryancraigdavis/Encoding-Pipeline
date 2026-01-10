//! Configuration validation system.

pub mod codec;
pub mod encoder_params;
pub mod paths;
pub mod report;
pub mod schema;
pub mod semantic;

use std::collections::HashSet;

use crate::config::model::AppConfig;
use crate::error::CapabilityError;

/// Severity level for validation issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    /// Blocks configuration loading.
    Error,
    /// Logged but allows loading.
    Warning,
}

/// A validation issue found during configuration checking.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// Severity of the issue.
    pub severity: ValidationSeverity,
    /// Path to the problematic config field (e.g., "profiles[0].audio.rules[2].codec").
    pub path: String,
    /// Description of the issue.
    pub message: String,
    /// Optional suggestion for fixing the issue.
    pub suggestion: Option<String>,
}

impl ValidationIssue {
    /// Creates a new error-level validation issue.
    pub fn error(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ValidationSeverity::Error,
            path: path.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    /// Creates a new warning-level validation issue.
    pub fn warning(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: ValidationSeverity::Warning,
            path: path.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    /// Adds a suggestion to this validation issue.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// Result of validating a configuration.
#[derive(Debug, Default)]
pub struct ValidationResult {
    issues: Vec<ValidationIssue>,
}

impl ValidationResult {
    /// Creates an empty validation result.
    pub fn new() -> Self {
        Self { issues: Vec::new() }
    }

    /// Adds an issue to the result.
    pub fn add(&mut self, issue: ValidationIssue) {
        self.issues.push(issue);
    }

    /// Extends the result with issues from another result.
    pub fn extend(&mut self, other: ValidationResult) {
        self.issues.extend(other.issues);
    }

    /// Returns true if there are no errors (warnings are allowed).
    pub fn is_valid(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == ValidationSeverity::Error)
    }

    /// Returns an iterator over error-level issues.
    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
    }

    /// Returns an iterator over warning-level issues.
    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Warning)
    }

    /// Returns the total number of issues.
    pub fn issue_count(&self) -> usize {
        self.issues.len()
    }

    /// Returns the number of errors.
    pub fn error_count(&self) -> usize {
        self.errors().count()
    }
}

/// System capabilities detected at startup.
#[derive(Debug, Clone)]
pub struct SystemCapabilities {
    /// Available FFmpeg encoders.
    pub available_encoders: HashSet<String>,
    /// Available FFmpeg decoders.
    pub available_decoders: HashSet<String>,
    /// Available av1an video encoders.
    pub av1an_encoders: HashSet<String>,
}

impl SystemCapabilities {
    /// Detects system capabilities by querying ffmpeg and av1an.
    pub fn detect() -> Result<Self, CapabilityError> {
        let available_encoders = detect_ffmpeg_encoders()?;
        let available_decoders = detect_ffmpeg_decoders()?;
        let av1an_encoders = detect_av1an_encoders()?;

        Ok(Self {
            available_encoders,
            available_decoders,
            av1an_encoders,
        })
    }
}

/// Detects available FFmpeg encoders by parsing `ffmpeg -encoders`.
fn detect_ffmpeg_encoders() -> Result<HashSet<String>, CapabilityError> {
    let output = std::process::Command::new("ffmpeg")
        .args(["-encoders", "-hide_banner"])
        .output()
        .map_err(|e| CapabilityError::CommandFailed {
            command: "ffmpeg -encoders".to_string(),
            message: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(CapabilityError::CommandFailed {
            command: "ffmpeg -encoders".to_string(),
            message: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let encoders = parse_ffmpeg_codec_list(&stdout);

    Ok(encoders)
}

/// Detects available FFmpeg decoders by parsing `ffmpeg -decoders`.
fn detect_ffmpeg_decoders() -> Result<HashSet<String>, CapabilityError> {
    let output = std::process::Command::new("ffmpeg")
        .args(["-decoders", "-hide_banner"])
        .output()
        .map_err(|e| CapabilityError::CommandFailed {
            command: "ffmpeg -decoders".to_string(),
            message: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(CapabilityError::CommandFailed {
            command: "ffmpeg -decoders".to_string(),
            message: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let decoders = parse_ffmpeg_codec_list(&stdout);

    Ok(decoders)
}

/// Parses FFmpeg encoder/decoder list output into a set of codec names.
fn parse_ffmpeg_codec_list(output: &str) -> HashSet<String> {
    let mut codecs = HashSet::new();

    for line in output.lines() {
        // Lines look like: " A..... aac                  AAC (Advanced Audio Coding)"
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.contains("Encoders:") {
            continue;
        }

        // Skip header lines
        if trimmed.starts_with("V") || trimmed.starts_with("A") || trimmed.starts_with("S") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                codecs.insert(parts[1].to_string());
            }
        }
    }

    codecs
}

/// Detects available av1an video encoders.
fn detect_av1an_encoders() -> Result<HashSet<String>, CapabilityError> {
    let mut encoders = HashSet::new();

    // Check for each supported encoder binary
    let encoder_checks = [
        ("x265", "x265"),
        ("x264", "x264"),
        ("SvtAv1EncApp", "svt-av1"),
        ("aomenc", "aomenc"),
        ("rav1e", "rav1e"),
    ];

    for (binary, name) in encoder_checks {
        if which_binary(binary).is_ok() {
            encoders.insert(name.to_string());
        }
    }

    // Also check if av1an itself is available
    if which_binary("av1an").is_err() {
        return Err(CapabilityError::ToolNotFound {
            tool: "av1an".to_string(),
        });
    }

    Ok(encoders)
}

/// Checks if a binary exists in PATH.
fn which_binary(name: &str) -> Result<std::path::PathBuf, CapabilityError> {
    let output = std::process::Command::new("which")
        .arg(name)
        .output()
        .map_err(|e| CapabilityError::CommandFailed {
            command: format!("which {}", name),
            message: e.to_string(),
        })?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(std::path::PathBuf::from(path))
    } else {
        Err(CapabilityError::ToolNotFound {
            tool: name.to_string(),
        })
    }
}

/// Validates the configuration against system capabilities.
pub fn validate_config(config: &AppConfig, capabilities: &SystemCapabilities) -> ValidationResult {
    let mut result = ValidationResult::new();

    // Run all validation layers
    result.extend(schema::validate(config));
    result.extend(semantic::validate(config));
    result.extend(codec::validate(config, capabilities));
    result.extend(paths::validate(config));

    // Validate encoder params for each profile
    for (i, profile) in config.profiles.iter().enumerate() {
        result.extend(encoder_params::validate(
            &profile.encoder,
            &profile.encoder_params,
            &format!("profiles[{}].encoder_params", i),
        ));
    }

    result
}
