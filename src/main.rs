mod commands;
mod config;
mod utils;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Report;
use commands::{farm::farm, init::init};
use std::fs::create_dir_all;
use tracing::instrument;
use tracing::level_filters::LevelFilter;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_error::ErrorLayer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, fmt::format::FmtSpan, EnvFilter, Layer};

const KEEP_LAST_N_DAYS: usize = 7;

#[derive(Debug, Parser)]
#[command(subcommand_required = true)]
#[command(arg_required_else_help = true)]
#[command(name = "subspace")]
#[command(about = "Subspace CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "initializes the config file required for the farming")]
    Init,
    #[command(about = "starting the farming process (along with node in the background")]
    Farm,
}

#[tokio::main]
#[instrument]
async fn main() -> Result<(), Report> {
    install_tracing();
    color_eyre::install()?;

    let args = Cli::parse();
    match args.command {
        Commands::Init => {
            init()?;
        }
        Commands::Farm => {
            farm().await?;
        }
    }

    Ok(())
}

fn install_tracing() {
    let log_dir = utils::custom_log_dir();
    let _ = create_dir_all(log_dir.clone());

    let mut file_appender = tracing_appender::rolling::daily(log_dir, "subspace-desktop.log");
    file_appender.keep_last_n_logs(KEEP_LAST_N_DAYS); // keep the logs of last 7 days only

    // filter for logging
    let filter = || {
        EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy()
            .add_directive("subspace_cli=debug".parse().unwrap())
    };

    // start logger, after we acquire the bundle identifier
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_ansi(!cfg!(windows))
                .with_span_events(FmtSpan::CLOSE)
                .with_filter(filter()),
        )
        .with(
            BunyanFormattingLayer::new("subspace-desktop".to_owned(), file_appender)
                .and_then(JsonStorageLayer)
                .with_filter(filter()),
        )
        .with(ErrorLayer::default())
        .init();
}
