mod commands;
mod config;
mod utils;

use clap::{Parser, Subcommand};
use color_eyre::eyre::Report;
use color_eyre::Help;
use commands::{farm::farm, init::init};
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

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "initializes the config file required for the farming")]
    Init,
    #[command(about = "starting the farming process (along with node in the background)")]
    Farm {
        #[arg(short, long, action)]
        verbose: bool,
    },
}

#[tokio::main]
#[instrument]
async fn main() -> Result<(), Report> {
    let args = Cli::parse();
    match args.command {
        Commands::Init => {
            init().suggestion(support_message())?;
        }
        Commands::Farm { verbose } => {
            farm(verbose).await.suggestion(support_message())?;
            // TODO: replace this with `farm.sync()` when it's ready on the SDK side
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }

    Ok(())
}
