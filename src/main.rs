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
        Commands::Wipe { mut farmer, mut node } => {
            // if user did not supply any argument, this means user wants to delete them both, but
            // `farmer` and `node` are both false at the moment
            if !farmer && !node {
                let prompt = "This will delete both farmer and node (complete wipe). Do you want \
                              to proceed? [y/n]";
                if let Ok(false) = get_user_input(prompt, None, yes_or_no_parser) {
                    println!("Wipe operation aborted, nothing has been deleted...");
                    return Ok(());
                }

                farmer = true;
                node = true;
            }
            wipe(farmer, node).await.suggestion(support_message())?;
        }
    }

    Ok(())
}
