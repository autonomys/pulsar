use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use color_eyre::eyre::{eyre, Report, Result};
use futures::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use single_instance::SingleInstance;
use tracing::instrument;

use subspace_sdk::{chain_spec, Farmer, Node, PlotDescription};

use crate::config::{validate_config, ChainConfig, Config};
use crate::summary::{create_summary_file, get_farmed_block_count, update_summary};
use crate::utils::install_tracing;

/// allows us to detect multiple instances of the farmer and act on it
pub(crate) const SINGLE_INSTANCE: &str = ".subspaceFarmer";

/// implementation of the `farm` command
///
/// takes `is_verbose`, returns a [`Farmer`], [`Node`], and a [`SingleInstance`]
///
/// first, checks for an existing farmer instance
/// then starts the farming and node instances,
/// lastly, depending on the verbosity, it subscribes to plotting progress and new solutions
#[instrument]
pub(crate) async fn farm(is_verbose: bool) -> Result<(Farmer, Node, SingleInstance)> {
    install_tracing(is_verbose);
    color_eyre::install()?;

    // TODO: this can be configured for chain in the future
    let instance = SingleInstance::new(SINGLE_INSTANCE)
        .map_err(|_| eyre!("Cannot take the instance lock from the OS! Aborting..."))?;
    if !instance.is_single() {
        return Err(eyre!(
            "It seems like there is already a farming instance running. Aborting...",
        ));
    }

    println!("Starting node ... (this might take up to couple of minutes)");
    let Config {
        chain,
        farmer: farmer_config,
        node: node_config,
    } = validate_config()?;

    let chain = match chain {
        ChainConfig::Gemini3a => {
            chain_spec::gemini_3a().expect("cannot extract the gemini3a chain spec from SDK")
        }
    };

    let node = node_config
        .node
        .build(node_config.directory, chain)
        .await
        .expect("error building the node");

    println!("Node started successfully!");

    create_summary_file(farmer_config.plot_size).await?;

    println!("Starting farmer ...");
    let farmer = farmer_config
        .farmer
        .build(
            farmer_config.address,
            node.clone(),
            &[PlotDescription::new(
                farmer_config.plot_directory,
                farmer_config.plot_size,
            )],
            farmer_config.cache,
        )
        .await?;

    println!("Farmer started successfully!");

    if !is_verbose {
        let is_initial_progress_finished = Arc::new(AtomicBool::new(false));
        let sector_size_bytes = farmer.get_info().await.map_err(Report::msg)?.sector_size;
        let farmer_clone = farmer.clone();
        let finished_flag = is_initial_progress_finished.clone();

        // initial plotting progress subscriber
        tokio::spawn(async move {
            for (plot_id, plot) in farmer_clone.iter_plots().await.enumerate() {
                println!(
                    "Initial plotting for plot: #{plot_id} ({})",
                    plot.directory().display()
                );
                let progress_bar = plotting_progress_bar(plot.allocated_space().as_u64());
                plot.subscribe_initial_plotting_progress()
                    .await
                    .for_each(|progress| {
                        let pb_clone = progress_bar.clone();
                        async move {
                            let current_bytes = progress.current_sector * sector_size_bytes;
                            pb_clone.set_position(current_bytes);
                        }
                    })
                    .await;
                progress_bar.set_style(
                    ProgressStyle::with_template(
                        "{spinner} [{elapsed_precise}] {percent}% [{bar:40.cyan/blue}]
                  ({bytes}/{total_bytes}) {msg}",
                    )
                    .unwrap()
                    .progress_chars("=> "),
                );
                progress_bar.finish_with_message("Initial plotting finished!");
                finished_flag.store(true, Ordering::Relaxed);
                let _ = update_summary(Some(true), None).await; // ignore the error, since we will abandon this mechanism soon
            }
        });

        // solution subscriber
        tokio::spawn({
            let farmer_clone = farmer.clone();

            let farmed_blocks = get_farmed_block_count()
                .await
                .expect("couldn't read farmed blocks count from summary");
            let farmed_block_count = Arc::new(AtomicU64::new(farmed_blocks));
            async move {
                for plot in farmer_clone.iter_plots().await {
                    plot.subscribe_new_solutions()
                        .await
                        .for_each(|_solution| async {
                            let total_farmed = farmed_block_count.fetch_add(1, Ordering::Relaxed);
                            let _ = update_summary(None, Some(total_farmed)).await; // ignore the error, since we will abandon this mechanism
                            if is_initial_progress_finished.load(Ordering::Relaxed) {
                                println!("You have farmed {total_farmed} block(s) in total!");
                            }
                        })
                        .await
                }
            }
        });
    }

    Ok((farmer, node, instance))
}

/// nice looking progress bar for the initial plotting :)
fn plotting_progress_bar(total_size: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] {percent}% [{bar:40.cyan/blue}]
      ({bytes}/{total_bytes}) {bytes_per_sec}, {msg}, ETA: {eta}",
        )
        .unwrap()
        .progress_chars("=> "),
    );
    pb.set_message("plotting");

    pb
}
