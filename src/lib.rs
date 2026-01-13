//! Encoding Pipeline - A VMAF-targeted video encoding pipeline using av1an.
//!
//! This library provides a complete video encoding solution with folder watching,
//! queue management, and configurable encoding profiles.

pub mod cli;
pub mod config;
pub mod encoder;
pub mod error;
pub mod media;
pub mod notify;
pub mod queue;
pub mod validation;
pub mod watcher;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::cli::{Cli, Commands, RunArgs};
use crate::config::ConfigManager;
use crate::encoder::EncodeWorker;
use crate::notify::{DiscordNotifier, MetricsServer};
use crate::queue::QueueManager;
use crate::validation::SystemCapabilities;
use crate::watcher::WatcherManager;

/// Runs the encoding pipeline with the provided CLI arguments.
pub async fn run(cli: Cli) -> Result<()> {
    setup_logging(&cli.log_level())?;

    match cli.command {
        Commands::Run(args) => run_pipeline(args, &cli.config).await,
        Commands::ConfigValidate => validate_config(&cli.config).await,
        Commands::ConfigShow => show_config(&cli.config).await,
        Commands::QueueList => list_queue(&cli.config).await,
        Commands::QueueClear => clear_queue(&cli.config).await,
        Commands::RetryDeadLetter { job_id } => retry_dead_letter(&cli.config, &job_id).await,
    }
}

/// Initializes the tracing subscriber for structured logging.
fn setup_logging(level: &str) -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .json()
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    Ok(())
}

/// Runs the main encoding pipeline loop.
async fn run_pipeline(args: RunArgs, config_path: &std::path::Path) -> Result<()> {
    info!("Starting encoding pipeline");

    // Detect system capabilities
    let capabilities = SystemCapabilities::detect()?;
    info!(?capabilities, "Detected system capabilities");

    // Load and validate config
    let config_manager = ConfigManager::new(config_path, &capabilities).await?;
    let config = config_manager.get_config();

    info!("Configuration loaded and validated");

    let config_read = config.read().await;

    // Build Redis URL
    let redis_url = build_redis_url(&config_read.global.redis);

    // Initialize Redis connection
    let queue = QueueManager::new(&redis_url).await?;
    info!("Connected to Redis");

    // Store config in Redis cache
    {
        let mut redis_conn = redis::Client::open(redis_url.as_str())?
            .get_connection_manager()
            .await?;
        config::cache::store_config(&mut redis_conn, &config_read).await?;
        info!("Configuration cached in Redis");
    }

    // Initialize metrics
    let metrics = Arc::new(notify::prometheus::Metrics::new()?);

    // Initialize Discord notifier if configured
    let discord = config_read
        .global
        .notifications
        .discord
        .as_ref()
        .map(|dc| Arc::new(DiscordNotifier::new(dc)));

    let prometheus_port = config_read.global.prometheus.port;
    let prometheus_enabled = config_read.global.prometheus.enabled;
    let stability_duration = Duration::from_secs(config_read.global.stability_check.duration_seconds);
    let poll_interval = Duration::from_secs(config_read.global.stability_check.poll_interval_seconds);
    let max_attempts = config_read.global.retry.max_attempts;
    let process_existing = args.process_existing;

    drop(config_read);

    // Start Prometheus metrics server
    if prometheus_enabled {
        let metrics_server = MetricsServer::new(metrics.clone(), prometheus_port);
        tokio::spawn(async move {
            if let Err(e) = metrics_server.start().await {
                error!(error = %e, "Prometheus server failed");
            }
        });
        info!(port = prometheus_port, "Prometheus metrics server started");
    }

    // Start config hot-reload watcher
    let (reload_tx, mut reload_rx) = mpsc::channel(10);
    let config_watcher = config::hot_reload::ConfigWatcher::new(
        config.clone(),
        config_path,
        capabilities.clone(),
        reload_tx,
    );
    tokio::spawn(async move {
        if let Err(e) = config_watcher.start().await {
            error!(error = %e, "Config watcher failed");
        }
    });
    info!("Config hot-reload enabled");

    // Start file watchers
    let mut watcher_manager = WatcherManager::new(
        config.clone(),
        queue.clone(),
        stability_duration,
        poll_interval,
    )
    .await;

    tokio::spawn(async move {
        if let Err(e) = watcher_manager.start(process_existing).await {
            error!(error = %e, "Watcher manager failed");
        }
    });
    info!("File watchers started");

    // Start encoder worker
    let (progress_tx, mut progress_rx) = mpsc::channel(100);
    let mut worker = EncodeWorker::new(
        queue.clone(),
        config.clone(),
        max_attempts,
        Some(progress_tx),
    );

    let metrics_clone = metrics.clone();
    tokio::spawn(async move {
        if let Err(e) = worker.run().await {
            error!(error = %e, "Encoder worker failed");
        }
    });
    info!("Encoder worker started");

    // Main loop: handle signals and events
    info!("Encoding pipeline is running. Press Ctrl+C to stop.");

    loop {
        tokio::select! {
            // Handle graceful shutdown
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received");
                break;
            }

            // Handle config reload events
            Some(event) = reload_rx.recv() => {
                match event {
                    config::hot_reload::ConfigReloadEvent::Reloaded => {
                        info!("Configuration reloaded");
                        // TODO: Signal watchers to update
                    }
                    config::hot_reload::ConfigReloadEvent::ValidationFailed { error_count } => {
                        warn!(error_count, "Configuration reload failed validation");
                    }
                }
            }

            // Handle progress updates (for metrics)
            Some(progress) = progress_rx.recv() => {
                // Update metrics
                metrics_clone.set_jobs_in_progress(1);
            }
        }
    }

    info!("Shutting down encoding pipeline");
    // TODO: Graceful shutdown - wait for current encode to complete
    Ok(())
}

/// Builds the Redis URL from configuration.
fn build_redis_url(config: &config::model::RedisConfig) -> String {
    match &config.password {
        Some(pass) => format!("redis://:{}@{}:{}/{}", pass, config.host, config.port, config.db),
        None => format!("redis://{}:{}/{}", config.host, config.port, config.db),
    }
}

/// Validates the configuration file and reports any issues.
async fn validate_config(config_path: &std::path::Path) -> Result<()> {
    let capabilities = SystemCapabilities::detect()?;
    let result = config::loader::load_and_validate(config_path, &capabilities)?;

    println!("Configuration is valid.");
    println!("Found {} profile(s):", result.profiles.len());
    for profile in &result.profiles {
        println!(
            "  - {} (encoder: {:?}, vmaf: {})",
            profile.name, profile.encoder, profile.vmaf_target
        );
    }

    Ok(())
}

/// Displays the parsed configuration.
async fn show_config(config_path: &std::path::Path) -> Result<()> {
    let capabilities = SystemCapabilities::detect()?;
    let config = config::loader::load_and_validate(config_path, &capabilities)?;
    let yaml = serde_yaml::to_string(&config)?;
    println!("{}", yaml);
    Ok(())
}

/// Lists all jobs in the queue.
async fn list_queue(config_path: &std::path::Path) -> Result<()> {
    let capabilities = SystemCapabilities::detect()?;
    let config = config::loader::load_and_validate(config_path, &capabilities)?;

    let redis_url = build_redis_url(&config.global.redis);
    let mut queue = QueueManager::new(&redis_url).await?;

    let jobs = queue.list_queue().await?;

    if jobs.is_empty() {
        println!("Queue is empty.");
    } else {
        println!("Queue ({} jobs):", jobs.len());
        for job in jobs {
            println!(
                "  {} - {} ({:?})",
                job.id,
                job.input_path.display(),
                job.status
            );
        }
    }

    let dead_letter = queue.list_dead_letter().await?;
    if !dead_letter.is_empty() {
        println!("\nDead letter queue ({} jobs):", dead_letter.len());
        for job in dead_letter {
            println!(
                "  {} - {} ({})",
                job.id,
                job.input_path.display(),
                job.error_message.as_deref().unwrap_or("Unknown error")
            );
        }
    }

    Ok(())
}

/// Clears all jobs from the queue.
async fn clear_queue(config_path: &std::path::Path) -> Result<()> {
    let capabilities = SystemCapabilities::detect()?;
    let config = config::loader::load_and_validate(config_path, &capabilities)?;

    let redis_url = build_redis_url(&config.global.redis);
    let mut queue = QueueManager::new(&redis_url).await?;

    let count = queue.clear_queue().await?;
    println!("Cleared {} job(s) from queue.", count);

    Ok(())
}

/// Retries a job from the dead letter queue.
async fn retry_dead_letter(config_path: &std::path::Path, job_id: &str) -> Result<()> {
    let capabilities = SystemCapabilities::detect()?;
    let config = config::loader::load_and_validate(config_path, &capabilities)?;

    let redis_url = build_redis_url(&config.global.redis);
    let mut queue = QueueManager::new(&redis_url).await?;

    queue.retry_dead_letter(job_id).await?;
    println!("Job {} moved from dead letter queue to main queue.", job_id);

    Ok(())
}
