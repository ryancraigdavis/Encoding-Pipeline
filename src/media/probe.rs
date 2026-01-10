//! FFprobe wrapper for media analysis.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Result of probing a media file.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// General media information.
    pub info: MediaInfo,
    /// Video streams in the file.
    pub video_streams: Vec<VideoStream>,
    /// Audio streams in the file.
    pub audio_streams: Vec<AudioStream>,
    /// Subtitle streams in the file.
    pub subtitle_streams: Vec<SubtitleStream>,
}

/// General media file information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    /// File path.
    pub path: String,
    /// Container format.
    pub format: String,
    /// Duration in seconds.
    pub duration: f64,
    /// File size in bytes.
    pub size: u64,
    /// Overall bitrate in bits per second.
    pub bitrate: u64,
}

/// Video stream information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoStream {
    /// Stream index.
    pub index: usize,
    /// Codec name.
    pub codec: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frame rate as a string (e.g., "24000/1001").
    pub frame_rate: String,
    /// Bit depth.
    pub bit_depth: u8,
    /// Color space.
    pub color_space: Option<String>,
    /// Color primaries.
    pub color_primaries: Option<String>,
    /// Color transfer characteristics.
    pub color_transfer: Option<String>,
    /// HDR format if applicable.
    pub hdr_format: Option<String>,
}

/// Audio stream information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStream {
    /// Stream index.
    pub index: usize,
    /// Codec name.
    pub codec: String,
    /// Number of channels.
    pub channels: u8,
    /// Channel layout (e.g., "5.1", "stereo").
    pub channel_layout: Option<String>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Bitrate in bits per second.
    pub bitrate: Option<u64>,
    /// Language code (ISO 639-2).
    pub language: Option<String>,
    /// Stream title.
    pub title: Option<String>,
    /// Whether this is the default track.
    pub is_default: bool,
    /// Whether this is a commentary track.
    pub is_commentary: bool,
    /// Whether this is for visually impaired.
    pub is_visual_impaired: bool,
}

/// Subtitle stream information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleStream {
    /// Stream index.
    pub index: usize,
    /// Codec name (e.g., "subrip", "hdmv_pgs_subtitle").
    pub codec: String,
    /// Language code (ISO 639-2).
    pub language: Option<String>,
    /// Stream title.
    pub title: Option<String>,
    /// Whether this is the default track.
    pub is_default: bool,
    /// Whether this is a forced track.
    pub is_forced: bool,
    /// Whether this is for hearing impaired (SDH).
    pub is_hearing_impaired: bool,
    /// Whether this is an image-based subtitle (PGS, VobSub).
    pub is_image_based: bool,
}

impl SubtitleStream {
    /// Returns true if this is a text-based subtitle.
    pub fn is_text_based(&self) -> bool {
        !self.is_image_based
    }
}

/// Probes a media file using ffprobe.
pub fn probe(path: &Path) -> Result<ProbeResult> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .context("Failed to run ffprobe")?;

    if !output.status.success() {
        anyhow::bail!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("Failed to parse ffprobe output")?;

    parse_probe_output(&json, path)
}

/// Parses ffprobe JSON output into structured data.
fn parse_probe_output(json: &serde_json::Value, path: &Path) -> Result<ProbeResult> {
    let format = json.get("format").context("Missing format in ffprobe output")?;
    let streams = json.get("streams")
        .and_then(|s| s.as_array())
        .context("Missing streams in ffprobe output")?;

    let info = MediaInfo {
        path: path.to_string_lossy().to_string(),
        format: format.get("format_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        duration: format.get("duration")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0),
        size: format.get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        bitrate: format.get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
    };

    let mut video_streams = Vec::new();
    let mut audio_streams = Vec::new();
    let mut subtitle_streams = Vec::new();

    for stream in streams {
        let codec_type = stream.get("codec_type").and_then(|v| v.as_str());

        match codec_type {
            Some("video") => {
                if let Some(vs) = parse_video_stream(stream) {
                    video_streams.push(vs);
                }
            }
            Some("audio") => {
                if let Some(aus) = parse_audio_stream(stream) {
                    audio_streams.push(aus);
                }
            }
            Some("subtitle") => {
                if let Some(ss) = parse_subtitle_stream(stream) {
                    subtitle_streams.push(ss);
                }
            }
            _ => {}
        }
    }

    Ok(ProbeResult {
        info,
        video_streams,
        audio_streams,
        subtitle_streams,
    })
}

/// Parses a video stream from ffprobe JSON.
fn parse_video_stream(stream: &serde_json::Value) -> Option<VideoStream> {
    Some(VideoStream {
        index: stream.get("index")?.as_u64()? as usize,
        codec: stream.get("codec_name")?.as_str()?.to_string(),
        width: stream.get("width")?.as_u64()? as u32,
        height: stream.get("height")?.as_u64()? as u32,
        frame_rate: stream.get("r_frame_rate")
            .and_then(|v| v.as_str())
            .unwrap_or("0/1")
            .to_string(),
        bit_depth: stream.get("bits_per_raw_sample")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(8),
        color_space: stream.get("color_space").and_then(|v| v.as_str()).map(String::from),
        color_primaries: stream.get("color_primaries").and_then(|v| v.as_str()).map(String::from),
        color_transfer: stream.get("color_transfer").and_then(|v| v.as_str()).map(String::from),
        hdr_format: detect_hdr_format(stream),
    })
}

/// Detects HDR format from stream properties.
fn detect_hdr_format(stream: &serde_json::Value) -> Option<String> {
    let transfer = stream.get("color_transfer").and_then(|v| v.as_str())?;

    match transfer {
        "smpte2084" => Some("HDR10".to_string()),
        "arib-std-b67" => Some("HLG".to_string()),
        _ => {
            // Check for Dolby Vision in side data
            if let Some(side_data) = stream.get("side_data_list").and_then(|v| v.as_array()) {
                for data in side_data {
                    if let Some(side_type) = data.get("side_data_type").and_then(|v| v.as_str()) {
                        if side_type.contains("Dolby Vision") {
                            return Some("Dolby Vision".to_string());
                        }
                    }
                }
            }
            None
        }
    }
}

/// Parses an audio stream from ffprobe JSON.
fn parse_audio_stream(stream: &serde_json::Value) -> Option<AudioStream> {
    let tags = stream.get("tags");
    let disposition = stream.get("disposition");

    let title = tags.and_then(|t| t.get("title")).and_then(|v| v.as_str()).map(String::from);
    let is_commentary = title.as_ref().map(|t| t.to_lowercase().contains("commentary")).unwrap_or(false)
        || disposition.and_then(|d| d.get("comment")).and_then(|v| v.as_i64()) == Some(1);

    Some(AudioStream {
        index: stream.get("index")?.as_u64()? as usize,
        codec: stream.get("codec_name")?.as_str()?.to_string(),
        channels: stream.get("channels")?.as_u64()? as u8,
        channel_layout: stream.get("channel_layout").and_then(|v| v.as_str()).map(String::from),
        sample_rate: stream.get("sample_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(48000),
        bitrate: stream.get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok()),
        language: tags.and_then(|t| t.get("language")).and_then(|v| v.as_str()).map(String::from),
        title,
        is_default: disposition.and_then(|d| d.get("default")).and_then(|v| v.as_i64()) == Some(1),
        is_commentary,
        is_visual_impaired: disposition.and_then(|d| d.get("visual_impaired")).and_then(|v| v.as_i64()) == Some(1),
    })
}

/// Parses a subtitle stream from ffprobe JSON.
fn parse_subtitle_stream(stream: &serde_json::Value) -> Option<SubtitleStream> {
    let tags = stream.get("tags");
    let disposition = stream.get("disposition");
    let codec = stream.get("codec_name")?.as_str()?;

    let is_image_based = matches!(codec, "hdmv_pgs_subtitle" | "dvd_subtitle" | "dvb_subtitle");

    Some(SubtitleStream {
        index: stream.get("index")?.as_u64()? as usize,
        codec: codec.to_string(),
        language: tags.and_then(|t| t.get("language")).and_then(|v| v.as_str()).map(String::from),
        title: tags.and_then(|t| t.get("title")).and_then(|v| v.as_str()).map(String::from),
        is_default: disposition.and_then(|d| d.get("default")).and_then(|v| v.as_i64()) == Some(1),
        is_forced: disposition.and_then(|d| d.get("forced")).and_then(|v| v.as_i64()) == Some(1),
        is_hearing_impaired: disposition.and_then(|d| d.get("hearing_impaired")).and_then(|v| v.as_i64()) == Some(1),
        is_image_based,
    })
}

/// Returns true if the codec represents a lossless audio format.
pub fn is_lossless_codec(codec: &str) -> bool {
    matches!(
        codec.to_lowercase().as_str(),
        "truehd" | "mlp" | "dts-hd ma" | "dtshd" | "flac" | "alac" | "pcm_s16le" | "pcm_s24le" | "pcm_s32le"
    )
}
