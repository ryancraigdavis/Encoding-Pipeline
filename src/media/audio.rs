//! Audio track selection and processing logic.

use crate::config::model::{AudioAction, AudioConfig, AudioMatchCriteria, AudioRule, TrackFlags};

use super::probe::AudioStream;

/// Represents a decision for an audio track.
#[derive(Debug, Clone)]
pub struct AudioDecision {
    /// The original stream.
    pub stream: AudioStream,
    /// Action to take.
    pub action: AudioTrackAction,
    /// Matched rule index (if any).
    pub matched_rule: Option<usize>,
}

/// Action to take for an audio track.
#[derive(Debug, Clone)]
pub enum AudioTrackAction {
    /// Copy the track as-is.
    Passthrough,
    /// Transcode to the specified codec and bitrate.
    Transcode {
        codec: String,
        bitrate: String,
    },
    /// Exclude the track.
    Exclude,
    /// Copy and add a stereo downmix.
    PassthroughWithDownmix {
        downmix_codec: String,
        downmix_bitrate: String,
    },
    /// Transcode and add a stereo downmix.
    TranscodeWithDownmix {
        codec: String,
        bitrate: String,
        downmix_codec: String,
        downmix_bitrate: String,
    },
}

/// Processes audio streams and determines what to do with each.
pub fn process_audio_streams(
    streams: &[AudioStream],
    config: &AudioConfig,
) -> Vec<AudioDecision> {
    let mut decisions = Vec::new();
    let mut track_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for stream in streams {
        let decision = match match_stream_to_rule(stream, config) {
            Some((rule_idx, rule)) => {
                // Check max tracks per language limit
                if let Some(max) = config.max_tracks_per_language {
                    if let Some(lang) = &stream.language {
                        let count = track_counts.entry(lang.clone()).or_insert(0);
                        if *count >= max {
                            AudioDecision {
                                stream: stream.clone(),
                                action: AudioTrackAction::Exclude,
                                matched_rule: Some(rule_idx),
                            }
                        } else {
                            *count += 1;
                            create_decision(stream, rule, rule_idx)
                        }
                    } else {
                        create_decision(stream, rule, rule_idx)
                    }
                } else {
                    create_decision(stream, rule, rule_idx)
                }
            }
            None => {
                // Apply fallback
                let action = match config.fallback {
                    crate::config::model::TrackFallback::Exclude => AudioTrackAction::Exclude,
                    crate::config::model::TrackFallback::Include |
                    crate::config::model::TrackFallback::Passthrough => AudioTrackAction::Passthrough,
                };
                AudioDecision {
                    stream: stream.clone(),
                    action,
                    matched_rule: None,
                }
            }
        };

        decisions.push(decision);
    }

    decisions
}

/// Matches a stream against audio rules and returns the first match.
fn match_stream_to_rule<'a>(
    stream: &AudioStream,
    config: &'a AudioConfig,
) -> Option<(usize, &'a AudioRule)> {
    for (idx, rule) in config.rules.iter().enumerate() {
        if matches_criteria(stream, &rule.match_criteria) {
            return Some((idx, rule));
        }
    }
    None
}

/// Checks if a stream matches the given criteria.
fn matches_criteria(stream: &AudioStream, criteria: &AudioMatchCriteria) -> bool {
    // Check language
    if let Some(lang) = &criteria.language {
        if stream.language.as_ref() != Some(lang) {
            return false;
        }
    }

    // Check multiple languages
    if let Some(languages) = &criteria.languages {
        if let Some(stream_lang) = &stream.language {
            if !languages.contains(stream_lang) {
                return false;
            }
        } else {
            return false;
        }
    }

    // Check codec
    if let Some(codec) = &criteria.codec {
        if stream.codec.to_lowercase() != codec.to_lowercase() {
            return false;
        }
    }

    // Check multiple codecs
    if let Some(codecs) = &criteria.codecs {
        if !codecs.iter().any(|c| c.to_lowercase() == stream.codec.to_lowercase()) {
            return false;
        }
    }

    // Check channel count
    if let Some(min) = criteria.channels_min {
        if stream.channels < min {
            return false;
        }
    }

    if let Some(max) = criteria.channels_max {
        if stream.channels > max {
            return false;
        }
    }

    // Check flags
    if let Some(flags) = &criteria.flags {
        if !matches_flags(stream, flags) {
            return false;
        }
    }

    // Check title contains
    if let Some(contains) = &criteria.title_contains {
        let contains_lower = contains.to_lowercase();
        let matches = stream
            .title
            .as_ref()
            .map(|t| t.to_lowercase().contains(&contains_lower))
            .unwrap_or(false);
        if !matches {
            return false;
        }
    }

    // Check index
    if let Some(index) = criteria.index {
        if stream.index != index {
            return false;
        }
    }

    true
}

/// Checks if a stream matches the given track flags.
fn matches_flags(stream: &AudioStream, flags: &TrackFlags) -> bool {
    if let Some(commentary) = flags.commentary {
        if stream.is_commentary != commentary {
            return false;
        }
    }

    if let Some(visual_impaired) = flags.visual_impaired {
        if stream.is_visual_impaired != visual_impaired {
            return false;
        }
    }

    if let Some(default) = flags.default {
        if stream.is_default != default {
            return false;
        }
    }

    true
}

/// Creates an audio decision from a matched rule.
fn create_decision(stream: &AudioStream, rule: &AudioRule, rule_idx: usize) -> AudioDecision {
    use crate::config::model::DownmixMode;

    let base_action = determine_base_action(stream, rule);
    let has_downmix = rule.downmix.as_ref().map(|d| !matches!(d.mode, DownmixMode::None)).unwrap_or(false);

    let action = if has_downmix && stream.channels > 2 {
        let downmix = rule.downmix.as_ref().unwrap();
        match base_action {
            AudioTrackAction::Passthrough => AudioTrackAction::PassthroughWithDownmix {
                downmix_codec: downmix.codec.clone(),
                downmix_bitrate: downmix.bitrate.clone(),
            },
            AudioTrackAction::Transcode { codec, bitrate } => AudioTrackAction::TranscodeWithDownmix {
                codec,
                bitrate,
                downmix_codec: downmix.codec.clone(),
                downmix_bitrate: downmix.bitrate.clone(),
            },
            other => other,
        }
    } else {
        base_action
    };

    AudioDecision {
        stream: stream.clone(),
        action,
        matched_rule: Some(rule_idx),
    }
}

/// Determines the base action (without downmix) for a stream.
fn determine_base_action(stream: &AudioStream, rule: &AudioRule) -> AudioTrackAction {
    match &rule.action {
        AudioAction::Passthrough => AudioTrackAction::Passthrough,
        AudioAction::Exclude => AudioTrackAction::Exclude,
        AudioAction::Transcode => {
            if let Some(transcode) = &rule.transcode {
                let bitrate = if super::probe::is_lossless_codec(&stream.codec) {
                    transcode.lossless_bitrate.as_ref().unwrap_or(&transcode.bitrate)
                } else {
                    &transcode.bitrate
                };
                AudioTrackAction::Transcode {
                    codec: transcode.codec.clone(),
                    bitrate: bitrate.clone(),
                }
            } else {
                AudioTrackAction::Passthrough
            }
        }
        AudioAction::PassthroughOrTranscode => {
            let should_passthrough = rule
                .passthrough_codecs
                .iter()
                .any(|c| c.to_lowercase() == stream.codec.to_lowercase());

            if should_passthrough {
                AudioTrackAction::Passthrough
            } else if let Some(transcode) = &rule.transcode {
                let bitrate = if super::probe::is_lossless_codec(&stream.codec) {
                    transcode.lossless_bitrate.as_ref().unwrap_or(&transcode.bitrate)
                } else {
                    &transcode.bitrate
                };
                AudioTrackAction::Transcode {
                    codec: transcode.codec.clone(),
                    bitrate: bitrate.clone(),
                }
            } else {
                AudioTrackAction::Passthrough
            }
        }
        AudioAction::PassthroughLossless => {
            if super::probe::is_lossless_codec(&stream.codec) {
                AudioTrackAction::Passthrough
            } else if let Some(transcode) = &rule.transcode {
                AudioTrackAction::Transcode {
                    codec: transcode.codec.clone(),
                    bitrate: transcode.bitrate.clone(),
                }
            } else {
                AudioTrackAction::Passthrough
            }
        }
    }
}
