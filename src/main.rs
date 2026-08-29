mod app;
mod cli;
mod collectors;
mod config;
mod engines;
mod format;
mod models;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    // Installed before the alternate screen is entered, so a panic anywhere
    // after this point still leaves a usable terminal.
    app::install_panic_hook();

    let cli = Cli::parse();
    let config = Config::load(cli)?;

    let mut app = app::App::new(config).await?;
    app.run().await?;

    Ok(())
}
