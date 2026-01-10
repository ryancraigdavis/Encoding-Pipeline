//! Discord webhook notifications.

use anyhow::Result;
use serde::Serialize;
use tracing::{error, info};

use crate::config::model::{DiscordConfig, DiscordEvents};
use crate::error::NotificationError;
use crate::queue::job::{EncodeJob, EncodeResultMetadata, JobStatus};

/// Sends notifications to Discord via webhook.
pub struct DiscordNotifier {
    /// Webhook URL.
    webhook_url: String,
    /// Event configuration.
    events: DiscordEvents,
    /// Optional user ID to mention on failures.
    mention_on_failure: Option<String>,
    /// HTTP client.
    client: reqwest::Client,
}

impl DiscordNotifier {
    /// Creates a new Discord notifier from config.
    pub fn new(config: &DiscordConfig) -> Self {
        Self {
            webhook_url: config.webhook_url.clone(),
            events: config.events.clone(),
            mention_on_failure: config.mention_on_failure.clone(),
            client: reqwest::Client::new(),
        }
    }

    /// Notifies about a completed encode.
    pub async fn notify_encode_success(&self, job: &EncodeJob) -> Result<(), NotificationError> {
        if !self.events.on_encode_success {
            return Ok(());
        }

        let metadata = job.result_metadata.as_ref();
        let size_reduction = metadata
            .map(|m| format!("{:.1}%", m.size_reduction_percent()))
            .unwrap_or_else(|| "N/A".to_string());

        let duration = metadata
            .map(|m| format_duration(m.encode_duration_secs))
            .unwrap_or_else(|| "N/A".to_string());

        let speed = metadata
            .map(|m| format!("{:.2}x", m.encoding_speed))
            .unwrap_or_else(|| "N/A".to_string());

        let embed = DiscordEmbed {
            title: "Encode Complete".to_string(),
            color: 0x00FF00, // Green
            fields: vec![
                EmbedField {
                    name: "File".to_string(),
                    value: job.input_path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".to_string()),
                    inline: false,
                },
                EmbedField {
                    name: "Profile".to_string(),
                    value: job.profile_name.clone(),
                    inline: true,
                },
                EmbedField {
                    name: "Size Reduction".to_string(),
                    value: size_reduction,
                    inline: true,
                },
                EmbedField {
                    name: "Duration".to_string(),
                    value: duration,
                    inline: true,
                },
                EmbedField {
                    name: "Speed".to_string(),
                    value: speed,
                    inline: true,
                },
            ],
        };

        self.send_embed(embed).await
    }

    /// Notifies about a failed encode.
    pub async fn notify_encode_failure(&self, job: &EncodeJob) -> Result<(), NotificationError> {
        if !self.events.on_encode_failure {
            return Ok(());
        }

        let error_msg = job.error_message.as_deref().unwrap_or("Unknown error");

        let mut content = String::new();
        if let Some(mention) = &self.mention_on_failure {
            content = mention.clone();
        }

        let embed = DiscordEmbed {
            title: "Encode Failed".to_string(),
            color: 0xFF0000, // Red
            fields: vec![
                EmbedField {
                    name: "File".to_string(),
                    value: job.input_path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".to_string()),
                    inline: false,
                },
                EmbedField {
                    name: "Profile".to_string(),
                    value: job.profile_name.clone(),
                    inline: true,
                },
                EmbedField {
                    name: "Attempt".to_string(),
                    value: job.attempt_count.to_string(),
                    inline: true,
                },
                EmbedField {
                    name: "Error".to_string(),
                    value: truncate(error_msg, 1024),
                    inline: false,
                },
            ],
        };

        self.send_embed_with_content(embed, &content).await
    }

    /// Notifies about a job moved to dead letter queue.
    pub async fn notify_dead_letter(&self, job: &EncodeJob) -> Result<(), NotificationError> {
        if !self.events.on_dead_letter {
            return Ok(());
        }

        let error_msg = job.error_message.as_deref().unwrap_or("Unknown error");

        let mut content = String::new();
        if let Some(mention) = &self.mention_on_failure {
            content = mention.clone();
        }

        let embed = DiscordEmbed {
            title: "Job Dead Lettered".to_string(),
            color: 0x800000, // Dark red
            fields: vec![
                EmbedField {
                    name: "File".to_string(),
                    value: job.input_path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".to_string()),
                    inline: false,
                },
                EmbedField {
                    name: "Job ID".to_string(),
                    value: job.id.clone(),
                    inline: true,
                },
                EmbedField {
                    name: "Attempts".to_string(),
                    value: job.attempt_count.to_string(),
                    inline: true,
                },
                EmbedField {
                    name: "Reason".to_string(),
                    value: truncate(error_msg, 1024),
                    inline: false,
                },
            ],
        };

        self.send_embed_with_content(embed, &content).await
    }

    /// Notifies that the queue is empty.
    pub async fn notify_queue_empty(&self) -> Result<(), NotificationError> {
        if !self.events.on_queue_empty {
            return Ok(());
        }

        let embed = DiscordEmbed {
            title: "Queue Empty".to_string(),
            color: 0x0088FF, // Blue
            fields: vec![
                EmbedField {
                    name: "Status".to_string(),
                    value: "All encoding jobs have been processed.".to_string(),
                    inline: false,
                },
            ],
        };

        self.send_embed(embed).await
    }

    /// Sends an embed to the Discord webhook.
    async fn send_embed(&self, embed: DiscordEmbed) -> Result<(), NotificationError> {
        self.send_embed_with_content(embed, "").await
    }

    /// Sends an embed with optional content text.
    async fn send_embed_with_content(
        &self,
        embed: DiscordEmbed,
        content: &str,
    ) -> Result<(), NotificationError> {
        let payload = DiscordPayload {
            content: if content.is_empty() { None } else { Some(content.to_string()) },
            embeds: vec![embed],
        };

        let response = self
            .client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "Discord webhook failed");
            return Err(NotificationError::DiscordFailed(format!(
                "HTTP {}: {}",
                status, text
            )));
        }

        info!("Discord notification sent");
        Ok(())
    }
}

/// Discord webhook payload.
#[derive(Serialize)]
struct DiscordPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    embeds: Vec<DiscordEmbed>,
}

/// Discord embed structure.
#[derive(Serialize)]
struct DiscordEmbed {
    title: String,
    color: u32,
    fields: Vec<EmbedField>,
}

/// Discord embed field.
#[derive(Serialize)]
struct EmbedField {
    name: String,
    value: String,
    inline: bool,
}

/// Formats a duration in seconds to a human-readable string.
fn format_duration(secs: f64) -> String {
    let hours = (secs / 3600.0) as u64;
    let minutes = ((secs % 3600.0) / 60.0) as u64;
    let seconds = (secs % 60.0) as u64;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Truncates a string to the specified length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
