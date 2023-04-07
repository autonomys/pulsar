use color_eyre::eyre::Result;
use owo_colors::OwoColorize;
use subspace_sdk::farmer::CacheDescription;
use subspace_sdk::{Node, PlotDescription};

use crate::config::{delete_config, parse_config};
use crate::summary::delete_summary;
use crate::utils::{cache_directory_getter, node_directory_getter, plot_directory_getter};

/// implementation of the `wipe` command
///
/// can wipe farmer, node, summary and plot
pub(crate) async fn wipe(
    wipe_farmer: bool,
    wipe_node: bool,
    wipe_summary: bool,
    wipe_config: bool,
) -> Result<()> {
    if wipe_node {
        println!("wiping node...");
        let node_directory = node_directory_getter();
        let _ = Node::wipe(node_directory).await;
    }

    if wipe_farmer {
        println!("wiping farmer...");
        let config = match parse_config() {
            Ok(args) => Some(args),
            Err(_) => {
                println!(
                    "could not read your config. Wipe will still continue... \n{}",
                    "However, if you have set a custom location for your plots, you will need to \
                     manually delete your plots!"
                        .underline()
                );
                None
            }
        };

        // TODO: modify here when supporting multi-plot
        // if config can be read, delete the farmer using the path in the config, else,
        // delete the default location
        if let Some(config) = config {
            match PlotDescription::new(config.farmer.plot_directory, config.farmer.plot_size) {
                Ok(plot) => {
                    let _ = plot.wipe().await;
                }
                Err(err) => println!(
                    "Skipping wiping plot. Got error while constructing the plot reference: {err}"
                ),
            }
            let _ =
                CacheDescription::new(cache_directory_getter(), config.farmer.advanced.cache_size)?
                    .wipe()
                    .await;
        } else {
            let _ = tokio::fs::remove_dir_all(plot_directory_getter()).await;
        }
    }

    if wipe_summary {
        delete_summary()?
    }

    if wipe_config {
        delete_config()?
    }

    println!("Wipe successful!");

    Ok(())
}
