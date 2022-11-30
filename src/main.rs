//! CLI application for farming
//! brings `farmer` and `node` together

#![deny(missing_docs)]

mod commands;
mod config;
mod summary;
mod utils;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Report;
use color_eyre::Help;
use commands::{farm::farm, info::info, init::init, wipe::wipe};
use tokio::signal;
use tracing::instrument;

use crate::utils::support_message;

#[derive(Debug, Parser)]
#[command(subcommand_required = true)]
#[command(arg_required_else_help = true)]
#[command(name = "subspace")]
#[command(about = "Subspace CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Available commands for the CLI
#[derive(Debug, Subcommand)]
enum Commands {
    #[command(
        about = "displays info about the farmer instance (i.e. total amount of rewards, and status of initial plotting)"
    )]
    Info,
    #[command(about = "initializes the config file required for the farming")]
    Init,
    #[command(about = "starting the farming process (along with node in the background)")]
    Farm {
        #[arg(short, long, action)]
        verbose: bool,
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
        Commands::Farm { verbose } => {
            let (farmer, node, _instance) = farm(verbose).await.suggestion(support_message())?;

            signal::ctrl_c().await?;
            println!("Will try to gracefully exit the application now. If you press ctrl+c again, it will try to forcefully close the app!");
            let handle = tokio::spawn(async {
                let _ = farmer.close().await;
                node.close().await;
            });
            tokio::select! {
                _ = handle => println!("gracefully closed the app!"),
                _ = signal::ctrl_c() => println!("forcefully closing the app!"),
            }
        }
        Commands::Wipe => {
            wipe().await?;
        }
    }

    Ok(())
}
