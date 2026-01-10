//! Subtitle track selection and processing logic.

use crate::config::model::{ImageSubsMode, SubtitleConfig, SubtitleTrackConfig, TrackFallback};

use super::probe::SubtitleStream;

/// Represents a decision for a subtitle track.
#[derive(Debug, Clone)]
pub struct SubtitleDecision {
    /// The original stream.
    pub stream: SubtitleStream,
    /// Action to take.
    pub action: SubtitleTrackAction,
}

/// Action to take for a subtitle track.
#[derive(Debug, Clone)]
pub enum SubtitleTrackAction {
    /// Copy the track as-is.
    Copy,
    /// Burn the subtitle into the video.
    BurnIn,
    /// Exclude the track.
    Exclude,
}

/// Processes subtitle streams and determines what to do with each.
pub fn process_subtitle_streams(
    streams: &[SubtitleStream],
    config: &SubtitleConfig,
) -> Vec<SubtitleDecision> {
    let mut decisions = Vec::new();

    for stream in streams {
        let decision = determine_subtitle_action(stream, config);
        decisions.push(decision);
    }

    decisions
}

/// Determines the action for a single subtitle stream.
fn determine_subtitle_action(stream: &SubtitleStream, config: &SubtitleConfig) -> SubtitleDecision {
    // Find matching track config
    let track_config = stream
        .language
        .as_ref()
        .and_then(|lang| config.tracks.iter().find(|t| &t.language == lang));

    let action = match track_config {
        Some(tc) => {
            // Check if this track type should be included
            let should_include = should_include_track(stream, tc);

            if !should_include {
                SubtitleTrackAction::Exclude
            } else if tc.burn_in && stream.is_image_based {
                SubtitleTrackAction::BurnIn
            } else {
                // Handle image-based subtitles according to global setting
                if stream.is_image_based {
                    match config.image_subs {
                        ImageSubsMode::Copy => SubtitleTrackAction::Copy,
                        ImageSubsMode::BurnIn => SubtitleTrackAction::BurnIn,
                        ImageSubsMode::Exclude => SubtitleTrackAction::Exclude,
                    }
                } else {
                    SubtitleTrackAction::Copy
                }
            }
        }
        None => {
            // No matching language config, use fallback
            match config.fallback {
                TrackFallback::Include | TrackFallback::Passthrough => {
                    if stream.is_image_based {
                        match config.image_subs {
                            ImageSubsMode::Copy => SubtitleTrackAction::Copy,
                            ImageSubsMode::BurnIn => SubtitleTrackAction::BurnIn,
                            ImageSubsMode::Exclude => SubtitleTrackAction::Exclude,
                        }
                    } else {
                        SubtitleTrackAction::Copy
                    }
                }
                TrackFallback::Exclude => SubtitleTrackAction::Exclude,
            }
        }
    };

    SubtitleDecision {
        stream: stream.clone(),
        action,
    }
}

/// Determines if a track should be included based on its type and config.
fn should_include_track(stream: &SubtitleStream, config: &SubtitleTrackConfig) -> bool {
    if stream.is_forced {
        return config.include_forced;
    }

    if stream.is_hearing_impaired {
        return config.include_sdh;
    }

    // Regular full subtitles
    config.include_full
}

/// Returns the subtitle stream that should be burned in, if any.
pub fn get_burn_in_stream(decisions: &[SubtitleDecision]) -> Option<&SubtitleStream> {
    decisions
        .iter()
        .find(|d| matches!(d.action, SubtitleTrackAction::BurnIn))
        .map(|d| &d.stream)
}

/// Returns all subtitle streams that should be copied.
pub fn get_copy_streams(decisions: &[SubtitleDecision]) -> Vec<&SubtitleStream> {
    decisions
        .iter()
        .filter(|d| matches!(d.action, SubtitleTrackAction::Copy))
        .map(|d| &d.stream)
        .collect()
}
