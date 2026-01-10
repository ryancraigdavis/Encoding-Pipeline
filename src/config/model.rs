//! Configuration data structures.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Root configuration structure containing all settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Global application settings.
    pub global: GlobalConfig,

    /// Encoding profiles with their associated watch folders.
    pub profiles: Vec<Profile>,
}

/// Global application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Logging level (trace, debug, info, warn, error).
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Temporary directory for intermediate files.
    #[serde(default = "default_temp_dir")]
    pub temp_dir: PathBuf,

    /// Redis connection settings.
    pub redis: RedisConfig,

    /// File stability detection settings.
    #[serde(default)]
    pub stability_check: StabilityConfig,

    /// Retry settings for failed encodes.
    #[serde(default)]
    pub retry: RetryConfig,

    /// Prometheus metrics settings.
    #[serde(default)]
    pub prometheus: PrometheusConfig,

    /// Notification settings.
    #[serde(default)]
    pub notifications: NotificationConfig,
}

/// Redis connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis server hostname.
    #[serde(default = "default_redis_host")]
    pub host: String,

    /// Redis server port.
    #[serde(default = "default_redis_port")]
    pub port: u16,

    /// Redis database number.
    #[serde(default)]
    pub db: u8,

    /// Optional Redis password.
    #[serde(default)]
    pub password: Option<String>,
}

/// File stability detection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityConfig {
    /// Duration in seconds the file size must remain stable.
    #[serde(default = "default_stability_duration")]
    pub duration_seconds: u64,

    /// Interval in seconds between stability checks.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_seconds: u64,
}

/// Retry configuration for failed encodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of attempts (1 = no retry, 2 = one retry).
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
}

/// Prometheus metrics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    /// Whether to enable Prometheus metrics endpoint.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Port for the Prometheus metrics HTTP server.
    #[serde(default = "default_prometheus_port")]
    pub port: u16,
}

/// Notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    /// Discord webhook settings.
    #[serde(default)]
    pub discord: Option<DiscordConfig>,
}

/// Discord webhook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Discord webhook URL.
    pub webhook_url: String,

    /// Which events trigger notifications.
    #[serde(default)]
    pub events: DiscordEvents,

    /// Optional user ID to mention on failures.
    #[serde(default)]
    pub mention_on_failure: Option<String>,
}

/// Discord notification event toggles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordEvents {
    /// Notify on successful encode completion.
    #[serde(default = "default_true")]
    pub on_encode_success: bool,

    /// Notify on encode failure.
    #[serde(default = "default_true")]
    pub on_encode_failure: bool,

    /// Notify when a job enters the dead letter queue.
    #[serde(default = "default_true")]
    pub on_dead_letter: bool,

    /// Notify when the queue becomes empty.
    #[serde(default)]
    pub on_queue_empty: bool,
}

/// An encoding profile with associated watch folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Unique name for this profile.
    pub name: String,

    /// Input directory to watch for new files.
    pub input_path: PathBuf,

    /// Output directory for encoded files.
    pub output_path: PathBuf,

    /// Whether to watch subdirectories recursively.
    #[serde(default = "default_true")]
    pub recursive: bool,

    /// File patterns to match (e.g., ["*.mkv", "*.mp4"]).
    #[serde(default = "default_file_patterns")]
    pub file_patterns: Vec<String>,

    /// Output file naming configuration.
    #[serde(default)]
    pub output_naming: OutputNaming,

    /// Video encoder to use.
    pub encoder: Encoder,

    /// Target VMAF score (0-100).
    #[serde(default = "default_vmaf_target")]
    pub vmaf_target: f32,

    /// Native encoder parameters.
    #[serde(default)]
    pub encoder_params: String,

    /// Number of av1an worker threads.
    #[serde(default = "default_workers")]
    pub workers: usize,

    /// Audio processing configuration.
    pub audio: AudioConfig,

    /// Subtitle processing configuration.
    pub subtitles: SubtitleConfig,
}

/// Output file naming configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputNaming {
    /// Directory structure: mirror input or flat.
    #[serde(default)]
    pub structure: OutputStructure,

    /// Filename handling: preserve or use template.
    #[serde(default)]
    pub filename: FilenameMode,

    /// Template string when filename mode is Template.
    #[serde(default)]
    pub template: Option<String>,

    /// Optional suffix to append to filename.
    #[serde(default)]
    pub suffix: Option<String>,
}

/// Output directory structure mode.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputStructure {
    /// Mirror the input directory structure.
    #[default]
    Mirror,
    /// Flatten all files into the output directory.
    Flat,
}

/// Filename handling mode.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FilenameMode {
    /// Preserve the original filename.
    #[default]
    Preserve,
    /// Use a template for naming.
    Template,
}

/// Supported video encoders.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Encoder {
    /// x265/HEVC encoder.
    X265,
    /// x264/AVC encoder.
    X264,
    /// SVT-AV1 encoder.
    #[serde(rename = "svt-av1")]
    SvtAv1,
    /// libaom AV1 encoder.
    Aomenc,
    /// rav1e AV1 encoder.
    Rav1e,
}

impl std::fmt::Display for Encoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::X265 => write!(f, "x265"),
            Self::X264 => write!(f, "x264"),
            Self::SvtAv1 => write!(f, "svt-av1"),
            Self::Aomenc => write!(f, "aomenc"),
            Self::Rav1e => write!(f, "rav1e"),
        }
    }
}

/// Audio processing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Audio processing rules, evaluated in order.
    pub rules: Vec<AudioRule>,

    /// How to handle tracks that match no rules.
    #[serde(default)]
    pub fallback: TrackFallback,

    /// Maximum tracks per language to keep.
    #[serde(default)]
    pub max_tracks_per_language: Option<usize>,

    /// Track ordering in output.
    #[serde(default)]
    pub output_order: OutputOrder,

    /// Language priority for ordering.
    #[serde(default)]
    pub language_priority: Vec<String>,
}

/// A rule for processing audio tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRule {
    /// Criteria for matching tracks.
    #[serde(rename = "match")]
    pub match_criteria: AudioMatchCriteria,

    /// Action to take on matched tracks.
    pub action: AudioAction,

    /// Codecs to pass through without transcoding.
    #[serde(default)]
    pub passthrough_codecs: Vec<String>,

    /// Transcode settings when action requires it.
    #[serde(default)]
    pub transcode: Option<TranscodeSettings>,

    /// Downmix settings.
    #[serde(default)]
    pub downmix: Option<DownmixSettings>,
}

/// Criteria for matching audio tracks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioMatchCriteria {
    /// Match specific language code (ISO 639-2).
    #[serde(default)]
    pub language: Option<String>,

    /// Match multiple language codes.
    #[serde(default)]
    pub languages: Option<Vec<String>>,

    /// Match specific codec.
    #[serde(default)]
    pub codec: Option<String>,

    /// Match multiple codecs.
    #[serde(default)]
    pub codecs: Option<Vec<String>>,

    /// Minimum channel count.
    #[serde(default)]
    pub channels_min: Option<u8>,

    /// Maximum channel count.
    #[serde(default)]
    pub channels_max: Option<u8>,

    /// Match track flags.
    #[serde(default)]
    pub flags: Option<TrackFlags>,

    /// Match title containing string (case-insensitive).
    #[serde(default)]
    pub title_contains: Option<String>,

    /// Match specific track index.
    #[serde(default)]
    pub index: Option<usize>,
}

/// Track flags for matching.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackFlags {
    /// Whether track is commentary.
    #[serde(default)]
    pub commentary: Option<bool>,

    /// Whether track is for visually impaired.
    #[serde(default)]
    pub visual_impaired: Option<bool>,

    /// Whether track is the default.
    #[serde(default)]
    pub default: Option<bool>,
}

/// Action to take on matched audio tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioAction {
    /// Copy track without modification.
    Passthrough,
    /// Always transcode the track.
    Transcode,
    /// Passthrough if codec is in list, otherwise transcode.
    PassthroughOrTranscode,
    /// Keep lossless codecs as-is.
    PassthroughLossless,
    /// Exclude track from output.
    Exclude,
}

/// Settings for audio transcoding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodeSettings {
    /// Target codec (aac, ac3, eac3, libopus, flac).
    pub codec: String,

    /// Target bitrate (e.g., "256k", "640k").
    pub bitrate: String,

    /// Bitrate for lossless sources (optional, uses bitrate if not set).
    #[serde(default)]
    pub lossless_bitrate: Option<String>,
}

/// Settings for audio downmixing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownmixSettings {
    /// Downmix mode.
    pub mode: DownmixMode,

    /// Codec for downmixed track.
    #[serde(default = "default_downmix_codec")]
    pub codec: String,

    /// Bitrate for downmixed track.
    #[serde(default = "default_downmix_bitrate")]
    pub bitrate: String,
}

/// Downmix mode options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownmixMode {
    /// No downmixing.
    None,
    /// Replace original with downmixed version.
    Replace,
    /// Add downmixed stereo track alongside original.
    AddStereo,
}

/// Subtitle processing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleConfig {
    /// Subtitle track configurations by language.
    pub tracks: Vec<SubtitleTrackConfig>,

    /// How to handle image-based subtitles (PGS, VobSub).
    #[serde(default)]
    pub image_subs: ImageSubsMode,

    /// How to handle tracks not matching any language rule.
    #[serde(default)]
    pub fallback: TrackFallback,

    /// Default track selection settings.
    #[serde(default)]
    pub default_track: Option<DefaultTrackConfig>,
}

/// Configuration for a specific subtitle language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleTrackConfig {
    /// Language code (ISO 639-2).
    pub language: String,

    /// Include forced subtitle tracks.
    #[serde(default = "default_true")]
    pub include_forced: bool,

    /// Include full subtitle tracks.
    #[serde(default = "default_true")]
    pub include_full: bool,

    /// Include SDH (hearing impaired) tracks.
    #[serde(default)]
    pub include_sdh: bool,

    /// Burn this language's subtitles into the video.
    #[serde(default)]
    pub burn_in: bool,
}

/// How to handle image-based subtitles.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageSubsMode {
    /// Copy image subtitles to output.
    #[default]
    Copy,
    /// Burn image subtitles into the video.
    BurnIn,
    /// Exclude image subtitles.
    Exclude,
}

/// Default track selection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultTrackConfig {
    /// Language for the default subtitle track.
    pub language: String,

    /// Prefer forced track as default if available.
    #[serde(default)]
    pub prefer_forced: bool,
}

/// Fallback behavior for unmatched tracks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrackFallback {
    /// Exclude unmatched tracks.
    #[default]
    Exclude,
    /// Include unmatched tracks.
    Include,
    /// Passthrough unmatched tracks without modification.
    Passthrough,
}

/// Track ordering in output.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputOrder {
    /// Preserve original track order.
    #[default]
    Preserve,
    /// Order by language priority.
    ByLanguagePriority,
}

// Default value functions

fn default_log_level() -> String {
    "info".to_string()
}

fn default_temp_dir() -> PathBuf {
    PathBuf::from("/tmp/encode_pipeline")
}

fn default_redis_host() -> String {
    "redis".to_string()
}

fn default_redis_port() -> u16 {
    6379
}

fn default_stability_duration() -> u64 {
    30
}

fn default_poll_interval() -> u64 {
    5
}

fn default_max_attempts() -> u32 {
    2
}

fn default_prometheus_port() -> u16 {
    9090
}

fn default_true() -> bool {
    true
}

fn default_file_patterns() -> Vec<String> {
    vec!["*.mkv".to_string()]
}

fn default_vmaf_target() -> f32 {
    93.0
}

fn default_workers() -> usize {
    4
}

fn default_downmix_codec() -> String {
    "aac".to_string()
}

fn default_downmix_bitrate() -> String {
    "160k".to_string()
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            duration_seconds: default_stability_duration(),
            poll_interval_seconds: default_poll_interval(),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
        }
    }
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: default_prometheus_port(),
        }
    }
}

impl Default for DiscordEvents {
    fn default() -> Self {
        Self {
            on_encode_success: true,
            on_encode_failure: true,
            on_dead_letter: true,
            on_queue_empty: false,
        }
    }
}
