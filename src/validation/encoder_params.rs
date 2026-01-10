//! Deep validation for encoder parameters.

use std::collections::HashSet;

use crate::config::model::Encoder;

use super::{ValidationIssue, ValidationResult, ValidationSeverity};

/// Known x265 parameters.
const X265_PARAMS: &[&str] = &[
    "preset", "tune", "crf", "qp", "bitrate", "pass", "stats", "slow-firstpass",
    "keyint", "min-keyint", "scenecut", "scenecut-bias", "hist-scenecut",
    "bframes", "b-adapt", "b-pyramid", "ref", "limit-refs",
    "deblock", "sao", "sao-non-deblock", "limit-sao",
    "aq-mode", "aq-strength", "qg-size", "cutree",
    "psy-rd", "psy-rdoq", "rdoq-level", "rd", "rskip", "rskip-edge-threshold",
    "colorprim", "transfer", "colormatrix", "chromaloc",
    "hdr10", "hdr10-opt", "dhdr10-info", "dhdr10-opt",
    "repeat-headers", "aud", "hrd", "info",
    "hash", "temporal-layers", "open-gop",
    "rc-lookahead", "lookahead-slices", "lookahead-threads",
    "pmode", "pme", "pools", "frame-threads", "wpp", "slices",
    "log-level", "csv", "csv-log-level",
    "sar", "overscan", "videoformat", "range", "master-display", "max-cll",
    "vbv-bufsize", "vbv-maxrate", "vbv-init", "crf-max", "crf-min",
    "ipratio", "pbratio", "qcomp", "qpstep", "qpmin", "qpmax",
    "cbqpoffs", "crqpoffs", "nr-intra", "nr-inter",
    "input-res", "input-depth", "input-csp", "interlace", "fps",
    "profile", "level-idc", "high-tier", "uhd-bd",
    "weightp", "weightb", "analyze-src-pics",
    "strong-intra-smoothing", "constrained-intra", "psy-rd",
    "rect", "amp", "early-skip", "fast-intra", "b-intra",
    "cu-lossless", "tskip-fast", "rd-refine",
    "max-merge", "me", "subme", "merange", "max-tu-size", "min-cu-size", "max-cu-size",
    "dynamic-rd", "ssim-rd",
];

/// Known x265 presets.
const X265_PRESETS: &[&str] = &[
    "ultrafast", "superfast", "veryfast", "faster", "fast",
    "medium", "slow", "slower", "veryslow", "placebo",
];

/// Known x265 tunes.
const X265_TUNES: &[&str] = &[
    "psnr", "ssim", "grain", "fastdecode", "zerolatency", "animation",
];

/// Known x264 parameters.
const X264_PARAMS: &[&str] = &[
    "preset", "tune", "profile", "level", "crf", "qp", "bitrate", "pass",
    "keyint", "min-keyint", "scenecut", "bframes", "b-adapt", "b-pyramid",
    "ref", "deblock", "aq-mode", "aq-strength", "psy-rd",
    "colorprim", "transfer", "colormatrix",
    "rc-lookahead", "mbtree", "threads", "lookahead-threads",
    "vbv-bufsize", "vbv-maxrate", "crf-max",
    "weightp", "weightb", "me", "subme", "merange",
    "direct", "trellis", "fast-pskip", "dct-decimate",
    "nr", "interlaced", "sar", "overscan", "videoformat", "range",
];

/// Known SVT-AV1 parameters.
const SVT_AV1_PARAMS: &[&str] = &[
    "preset", "crf", "qp", "keyint", "lookahead", "tile-rows", "tile-columns",
    "enable-overlays", "scd", "film-grain", "film-grain-denoise",
    "enable-qm", "qm-min", "qm-max", "hierarchical-levels",
    "pred-struct", "enable-dlf", "enable-cdef", "enable-restoration",
    "enable-tpl-la", "enable-tf", "enable-overlays",
    "aq-mode", "lp", "pin", "ss", "irefresh-type",
    "color-primaries", "transfer-characteristics", "matrix-coefficients",
    "color-range", "chroma-sample-position",
    "mastering-display", "content-light",
    "input-depth", "profile", "level", "tier", "fast-decode",
];

/// Validates encoder parameters for the given encoder.
pub fn validate(encoder: &Encoder, params: &str, path: &str) -> ValidationResult {
    let mut result = ValidationResult::new();

    if params.is_empty() {
        return result;
    }

    match encoder {
        Encoder::X265 => validate_x265_params(params, path, &mut result),
        Encoder::X264 => validate_x264_params(params, path, &mut result),
        Encoder::SvtAv1 => validate_svt_av1_params(params, path, &mut result),
        Encoder::Aomenc | Encoder::Rav1e => {
            // For less common encoders, just do basic syntax check
            validate_param_syntax(params, path, &mut result);
        }
    }

    result
}

/// Validates x265 encoder parameters.
fn validate_x265_params(params: &str, path: &str, result: &mut ValidationResult) {
    let known: HashSet<&str> = X265_PARAMS.iter().copied().collect();
    let parsed = parse_params(params);

    for param in parsed {
        if !known.contains(param.name.as_str()) {
            let suggestion = find_similar_param(&param.name, &known);
            result.add(
                ValidationIssue {
                    severity: ValidationSeverity::Warning,
                    path: format!("{}.{}", path, param.name),
                    message: format!("Unknown x265 parameter: '--{}'", param.name),
                    suggestion: Some(format!("Did you mean '--{}'?", suggestion)),
                },
            );
            continue;
        }

        // Validate specific parameter values
        if let Some(value) = &param.value {
            validate_x265_param_value(&param.name, value, path, result);
        }
    }
}

/// Validates a specific x265 parameter value.
fn validate_x265_param_value(name: &str, value: &str, path: &str, result: &mut ValidationResult) {
    match name {
        "preset" => {
            if !X265_PRESETS.contains(&value) {
                result.add(
                    ValidationIssue::error(
                        format!("{}.preset", path),
                        format!("Invalid x265 preset: '{}'", value),
                    )
                    .with_suggestion(format!("Valid presets: {}", X265_PRESETS.join(", "))),
                );
            }
        }
        "tune" => {
            if !X265_TUNES.contains(&value) {
                result.add(
                    ValidationIssue::error(
                        format!("{}.tune", path),
                        format!("Invalid x265 tune: '{}'", value),
                    )
                    .with_suggestion(format!("Valid tunes: {}", X265_TUNES.join(", "))),
                );
            }
        }
        "crf" => {
            if let Ok(crf) = value.parse::<f32>() {
                if !(0.0..=51.0).contains(&crf) {
                    result.add(
                        ValidationIssue::error(
                            format!("{}.crf", path),
                            format!("CRF {} is out of range", crf),
                        )
                        .with_suggestion("CRF must be between 0 and 51"),
                    );
                }
            } else {
                result.add(ValidationIssue::error(
                    format!("{}.crf", path),
                    format!("Invalid CRF value: '{}'", value),
                ));
            }
        }
        "bframes" => {
            if let Ok(bf) = value.parse::<u32>() {
                if bf > 16 {
                    result.add(
                        ValidationIssue::warning(
                            format!("{}.bframes", path),
                            format!("bframes {} is unusually high", bf),
                        )
                        .with_suggestion("Typical values are 3-8"),
                    );
                }
            }
        }
        "ref" => {
            if let Ok(r) = value.parse::<u32>() {
                if r > 16 {
                    result.add(
                        ValidationIssue::warning(
                            format!("{}.ref", path),
                            format!("ref {} is unusually high", r),
                        )
                        .with_suggestion("Typical values are 3-6"),
                    );
                }
            }
        }
        _ => {}
    }
}

/// Validates x264 encoder parameters.
fn validate_x264_params(params: &str, path: &str, result: &mut ValidationResult) {
    let known: HashSet<&str> = X264_PARAMS.iter().copied().collect();
    let parsed = parse_params(params);

    for param in parsed {
        if !known.contains(param.name.as_str()) {
            let suggestion = find_similar_param(&param.name, &known);
            result.add(ValidationIssue {
                severity: ValidationSeverity::Warning,
                path: format!("{}.{}", path, param.name),
                message: format!("Unknown x264 parameter: '--{}'", param.name),
                suggestion: Some(format!("Did you mean '--{}'?", suggestion)),
            });
        }
    }
}

/// Validates SVT-AV1 encoder parameters.
fn validate_svt_av1_params(params: &str, path: &str, result: &mut ValidationResult) {
    let known: HashSet<&str> = SVT_AV1_PARAMS.iter().copied().collect();
    let parsed = parse_params(params);

    for param in parsed {
        if !known.contains(param.name.as_str()) {
            let suggestion = find_similar_param(&param.name, &known);
            result.add(ValidationIssue {
                severity: ValidationSeverity::Warning,
                path: format!("{}.{}", path, param.name),
                message: format!("Unknown SVT-AV1 parameter: '--{}'", param.name),
                suggestion: Some(format!("Did you mean '--{}'?", suggestion)),
            });
        }
    }
}

/// Validates basic parameter syntax without checking specific encoder.
fn validate_param_syntax(params: &str, path: &str, result: &mut ValidationResult) {
    // Just check that it looks like valid CLI params
    let parsed = parse_params(params);

    if parsed.is_empty() && !params.trim().is_empty() {
        result.add(
            ValidationIssue::warning(path, "Could not parse encoder parameters")
                .with_suggestion("Parameters should be in '--key value' format"),
        );
    }
}

/// A parsed parameter with name and optional value.
struct ParsedParam {
    name: String,
    value: Option<String>,
}

/// Parses a parameter string into individual parameters.
fn parse_params(params: &str) -> Vec<ParsedParam> {
    let mut result = Vec::new();
    let mut chars = params.chars().peekable();

    while chars.peek().is_some() {
        // Skip whitespace
        while chars.peek() == Some(&' ') {
            chars.next();
        }

        // Check for -- prefix
        if chars.next() != Some('-') {
            continue;
        }
        if chars.next() != Some('-') {
            continue;
        }

        // Read parameter name
        let mut name = String::new();
        while let Some(&c) = chars.peek() {
            if c == ' ' || c == '=' {
                break;
            }
            name.push(c);
            chars.next();
        }

        if name.is_empty() {
            continue;
        }

        // Read value if present
        let value = if chars.peek() == Some(&'=') {
            chars.next(); // consume '='
            let mut val = String::new();
            while let Some(&c) = chars.peek() {
                if c == ' ' && !val.starts_with('"') {
                    break;
                }
                val.push(c);
                chars.next();
            }
            Some(val)
        } else if chars.peek() == Some(&' ') {
            chars.next(); // consume space
            // Peek ahead to see if next thing is another param
            if chars.peek() == Some(&'-') {
                None
            } else {
                let mut val = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' ' {
                        break;
                    }
                    val.push(c);
                    chars.next();
                }
                if val.is_empty() { None } else { Some(val) }
            }
        } else {
            None
        };

        result.push(ParsedParam { name, value });
    }

    result
}

/// Finds the most similar parameter name using Levenshtein distance.
fn find_similar_param<'a>(input: &str, known: &HashSet<&'a str>) -> &'a str {
    known
        .iter()
        .min_by_key(|p| strsim::levenshtein(input, p))
        .copied()
        .unwrap_or("preset")
}
