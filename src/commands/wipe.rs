use color_eyre::eyre::{Context, Result};
use owo_colors::OwoColorize;
use subspace_sdk::{Node, PlotDescription};

use crate::config::{delete_config, parse_config};
use crate::summary::delete_summary;
use crate::utils::{
    get_user_input, node_directory_getter, plot_directory_getter, yes_or_no_parser,
};

/// wipe configurator
///
/// sets the `farmer`, `node`, `summary`, and `config` flags for the `wipe`
/// command
pub(crate) async fn wipe_config(farmer: bool, node: bool) -> Result<()> {
    if !farmer && !node {
        // if user did not supply any argument, ask for everything
        let prompt = "Do you want to wipe farmer (delete plot)? [y/n]: ";
        let wipe_farmer =
            get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

        let prompt = "Do you want to wipe node? [y/n]: ";
        let wipe_node = get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

        let prompt = "Do you want to wipe summary? [y/n]: ";
        let wipe_summary =
            get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

        let prompt = "Do you want to wipe config? [y/n]: ";
        let wipe_config =
            get_user_input(prompt, None, yes_or_no_parser).context("prompt failed")?;

        wipe(wipe_farmer, wipe_node, wipe_summary, wipe_config).await?;
    } else {
        // don't delete summary and config if user supplied flags
        wipe(farmer, node, false, false).await?;
    }

    Ok(())
}

/// implementation of the `wipe` command
///
/// can wipe farmer, node, summary and plot
async fn wipe(
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
            let _ = PlotDescription::new(config.farmer.plot_directory, config.farmer.plot_size)
                .wipe()
                .await;
        } else {
            let _ = tokio::fs::remove_dir_all(plot_directory_getter()).await;
        }
    }

    if wipe_summary {
        match delete_summary() {
            Ok(_) => println!("deleted the summary file"),
            Err(_) => println!("Skipping wiping summary, could not find the file..."),
        }
    }

    if wipe_config {
        match delete_config() {
            Ok(_) => println!("deleted the config file"),
            Err(_) => println!("Skipping wiping config, could not find the file..."),
        }
    }

    println!("Wipe finished!");

    Ok(())
}
