use anyhow::Result;
use clap::Parser;
use encoding_pipeline::{cli::Cli, run};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli).await
}
