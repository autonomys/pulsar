//! CLI application for farming
//! brings `farmer` and `node` together

#![deny(missing_docs, clippy::unwrap_used)]
#![feature(concat_idents)]

mod commands;
mod config;
mod summary;
mod utils;

#[cfg(test)]
mod tests;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Report};
use color_eyre::Help;
use tracing::instrument;

use crate::commands::farm::farm;
use crate::commands::info::info;
use crate::commands::init::init;
use crate::commands::wipe::wipe;
use crate::utils::{get_user_input, support_message, yes_or_no_parser};

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
    Wipe {
        #[arg(long, action)]
        farmer: bool,
        #[arg(long, action)]
        node: bool,
    },
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
        Commands::Wipe { farmer, node } => {
            if !farmer && !node {
                // if user did not supply any argument, ask for everything
                let prompt = "Do you want to wipe farmer (delete plot)? [y/n]: ";
                let wipe_farmer =
                    get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

                let prompt = "Do you want to wipe node? [y/n]: ";
                let wipe_node =
                    get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

                let prompt = "Do you want to wipe summary? [y/n]: ";
                let wipe_summary =
                    get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

                let prompt = "Do you want to wipe config? [y/n]: ";
                let wipe_config =
                    get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

                wipe(wipe_farmer, wipe_node, wipe_summary, wipe_config)
                    .await
                    .suggestion(support_message())?;
            } else {
                // don't delete summary and config if user supplied flags
                wipe(farmer, node, false, false).await.suggestion(support_message())?;
            }
        }
    }

    Ok(())
}
