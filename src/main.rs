mod app;
mod cli;
mod collectors;
mod config;
mod engines;
mod models;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let config = Config::load(cli)?;

    let mut app = app::App::new(config).await?;
    app.run().await?;

    Ok(())
}