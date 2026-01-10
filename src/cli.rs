//! Command-line interface definitions.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// A VMAF-targeted video encoding pipeline using av1an.
#[derive(Parser, Debug)]
#[command(name = "encode-pipeline", version, about, long_about = None)]
pub struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "/config/pipeline.yaml", env = "CONFIG_PATH", global = true)]
    pub config: PathBuf,

    /// Increase logging verbosity (-v, -vv, -vvv).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    /// Returns the log level based on verbosity flags.
    pub fn log_level(&self) -> &'static str {
        match self.verbose {
            0 => "info",
            1 => "debug",
            _ => "trace",
        }
    }
}

/// Available subcommands for the encoding pipeline.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the encoding pipeline and watch for new files.
    Run(RunArgs),

    /// Validate the configuration file without starting.
    #[command(name = "config-validate")]
    ConfigValidate,

    /// Display the parsed configuration.
    #[command(name = "config-show")]
    ConfigShow,

    /// List all jobs currently in the queue.
    #[command(name = "queue-list")]
    QueueList,

    /// Clear all jobs from the queue.
    #[command(name = "queue-clear")]
    QueueClear,

    /// Retry a job from the dead letter queue.
    #[command(name = "retry-dead-letter")]
    RetryDeadLetter {
        /// The job ID to retry.
        job_id: String,
    },
}

/// Arguments for the run subcommand.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// Run in dry-run mode (no actual encoding).
    #[arg(long, default_value = "false")]
    pub dry_run: bool,

    /// Process existing files in watch folders on startup.
    #[arg(long, default_value = "false")]
    pub process_existing: bool,
}
