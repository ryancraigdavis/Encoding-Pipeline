//! Validation report formatting.

use super::{ValidationIssue, ValidationResult, ValidationSeverity};

/// Formats a validation result into a human-readable report.
pub fn format_report(result: &ValidationResult) -> String {
    let errors: Vec<_> = result.errors().collect();
    let warnings: Vec<_> = result.warnings().collect();

    if errors.is_empty() && warnings.is_empty() {
        return "Configuration is valid.".to_string();
    }

    let mut report = String::new();

    if !errors.is_empty() {
        report.push_str("\nConfig Validation Failed\n");
        report.push_str("========================\n\n");
    }

    // Format errors first
    for issue in &errors {
        report.push_str(&format_issue(issue));
        report.push('\n');
    }

    // Then warnings
    if !warnings.is_empty() {
        if !errors.is_empty() {
            report.push_str("\nWarnings:\n");
            report.push_str("---------\n\n");
        }
        for issue in &warnings {
            report.push_str(&format_issue(issue));
            report.push('\n');
        }
    }

    // Summary line
    report.push_str("---\n");
    report.push_str(&format!(
        "{} warning(s), {} error(s)\n",
        warnings.len(),
        errors.len()
    ));

    if !errors.is_empty() {
        report.push_str("Config rejected. Current config unchanged.\n");
    }

    report
}

/// Formats a single validation issue.
fn format_issue(issue: &ValidationIssue) -> String {
    let prefix = match issue.severity {
        ValidationSeverity::Error => "ERROR",
        ValidationSeverity::Warning => "WARNING",
    };

    let mut output = format!("{} {}\n", prefix, issue.path);
    output.push_str(&format!("  └─ {}\n", issue.message));

    if let Some(suggestion) = &issue.suggestion {
        output.push_str(&format!("     {}\n", suggestion));
    }

    output
}

/// Formats a brief summary suitable for notifications.
pub fn format_brief_summary(result: &ValidationResult) -> String {
    let error_count = result.error_count();
    let warning_count = result.warnings().count();

    if error_count == 0 && warning_count == 0 {
        "Configuration valid".to_string()
    } else if error_count == 0 {
        format!("Configuration valid with {} warning(s)", warning_count)
    } else {
        format!("Configuration invalid: {} error(s), {} warning(s)", error_count, warning_count)
    }
}
