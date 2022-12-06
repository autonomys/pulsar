use color_eyre::eyre::Result;

use subspace_sdk::{Node, PlotDescription};

use crate::config::parse_config;
use crate::summary::delete_summary;
use crate::utils::node_directory_getter;

/// implementation of the `wipe` command
///
/// wipes both farmer and node files (basically a fresh start)
pub(crate) async fn wipe() -> Result<()> {
    let config = match parse_config() {
        Ok(args) => args,
        Err(_) => {
            println!("could not read your config. You must have a valid config in order to wipe. Aborting...");
            return Ok(());
        }
    };
    let node_directory = node_directory_getter();
    let _ = Node::wipe(node_directory).await;
    println!("Node is wiped!");

    // TODO: modify here when supporting multi-plot
    let plot = PlotDescription {
        directory: config.farmer.plot_directory,
        space_pledged: config.farmer.plot_size,
    };

    let _ = plot.wipe().await;
    let _ = config.farmer.cache.wipe().await;
    println!("Farmer is wiped!");

    delete_summary().await;

    Ok(())
}
