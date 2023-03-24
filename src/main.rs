//! CLI application for farming
//! brings `farmer` and `node` together

#![deny(missing_docs, clippy::unwrap_used)]
#![feature(concat_idents)]

mod commands;
mod config;
mod summary;
mod utils;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Report;
use color_eyre::Help;
use commands::farm::farm;
use commands::info::info;
use commands::init::init;
use commands::wipe::wipe;
use tracing::instrument;

use crate::utils::support_message;

#[cfg(all(
    target_arch = "x86_64",
    target_vendor = "unknown",
    target_os = "linux",
    target_env = "gnu"
))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[derive(Debug, Parser)]
#[command(subcommand_required = true)]
#[command(arg_required_else_help = true)]
#[command(name = "subspace")]
#[command(about = "Subspace CLI", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Available commands for the CLI
#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "displays info about the farmer instance (i.e. total amount of rewards, \
                       and status of initial plotting)")]
    Info,
    #[command(about = "initializes the config file required for the farming")]
    Init,
    #[command(about = "starting the farming process (along with node in the background)")]
    Farm {
        #[arg(short, long, action)]
        verbose: bool,
        #[arg(short, long, action)]
        executor: bool,
    },
    #[command(about = "wipes the node and farm instance (along with your plots)")]
    Wipe,
}

#[tokio::main]
#[instrument]
async fn main() -> Result<(), Report> {
    let args = Cli::parse();
    match args.command {
        Commands::Info => {
            info().await?;
        }
        Commands::Init => {
            init().suggestion(support_message())?;
        }
        Commands::Farm { verbose, executor } => {
            farm(verbose, executor).await.suggestion(support_message())?;
        }
        Commands::Wipe => {
            wipe().await?;
        }
    }

    Ok(())
}
