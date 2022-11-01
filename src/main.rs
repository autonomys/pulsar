mod commands;
mod config;
mod utils;

use clap::Command;
use commands::{farm::farm, init::init};
use std::fs::create_dir_all;
use tracing::level_filters::LevelFilter;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, fmt::format::FmtSpan, EnvFilter, Layer};

const KEEP_LAST_N_DAYS: usize = 7;

fn cli() -> Command {
    Command::new("subspace")
        .about("Subspace CLI interface")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("init").about("initializes the config file required for the farming"),
        )
        .subcommand(
            Command::new("farm")
                .about("starting the farming process (along with node in the background)"),
        )
}

#[tokio::main]
async fn main() {
    let log_dir = utils::custom_log_dir();
    create_dir_all(log_dir.clone()).expect("path creation should always succeed");

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
        .init();

    let command = cli();
    let matches = command.get_matches();
    match matches.subcommand() {
        Some(("init", _)) => {
            init();
        }
        Some(("farm", _)) => {
            farm().await;
        }
        _ => unreachable!(), // all commands are defined above
    }
}
