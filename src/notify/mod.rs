//! Notification system for Discord webhooks and Prometheus metrics.

pub mod discord;
pub mod prometheus;

pub use discord::DiscordNotifier;
pub use prometheus::MetricsServer;
