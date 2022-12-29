use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use color_eyre::eyre::{eyre, Report, Result, WrapErr};
use futures::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use single_instance::SingleInstance;
use subspace_sdk::node::SyncingProgress;
use subspace_sdk::{chain_spec, Farmer, Node, PlotDescription};
use tracing::instrument;

use crate::config::{validate_config, ChainConfig, Config};
use crate::summary::Summary;
use crate::utils::{install_tracing, raise_fd_limit};

/// allows us to detect multiple instances of the farmer and act on it
pub(crate) const SINGLE_INSTANCE: &str = ".subspaceFarmer";

/// implementation of the `farm` command
///
/// takes `is_verbose`, returns a [`Farmer`], [`Node`], and a [`SingleInstance`]
///
/// first, checks for an existing farmer instance
/// then starts the farming and node instances,
/// lastly, depending on the verbosity, it subscribes to plotting progress and
/// new solutions
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
    // raise file limit
    raise_fd_limit();

    let Config { chain, farmer: farmer_config, node: node_config } = validate_config()?;

    println!("Starting node ...");
    let chain = match chain {
        ChainConfig::Gemini3b =>
            chain_spec::gemini_3b().expect("cannot extract the gemini3b chain spec from SDK"),
    };

    let node = node_config
        .node
        .build(node_config.directory, chain)
        .await
        .expect("error building the node");

    println!("Node started successfully!");

    if !is_verbose {
        subscribe_to_node_syncing(node.clone()).await?;
    } else {
        node.sync().await.map_err(|err| eyre!("Node syncing failed: {err}"))?;
    }

    let summary = Summary::new(Some(farmer_config.plot_size)).await?;

    println!("Starting farmer ...");
    let farmer = farmer_config
        .farmer
        .build(
            farmer_config.address,
            node.clone(),
            &[PlotDescription::new(farmer_config.plot_directory, farmer_config.plot_size)
                .wrap_err("Plot size is too low")?],
            farmer_config.cache,
        )
        .await?;

    println!("Farmer started successfully!");

    if !is_verbose {
        let is_initial_progress_finished = Arc::new(AtomicBool::new(false));
        let sector_size_bytes = farmer.get_info().await.map_err(Report::msg)?.sector_size;
        subscribe_to_plotting_progress(
            summary.clone(),
            farmer.clone(),
            is_initial_progress_finished.clone(),
            sector_size_bytes,
        )
        .await;
        subscribe_to_solutions(
            summary.clone(),
            farmer.clone(),
            is_initial_progress_finished.clone(),
        )
        .await;
    }

    Ok((farmer, node, instance))
}

async fn subscribe_to_node_syncing(node: Node) -> Result<()> {
    let mut syncing_progress = node
        .subscribe_syncing_progress()
        .await
        .map_err(|err| eyre!("Failed to subscribe to node syncing: {err}"))?
        .map_ok(|SyncingProgress { at, target, status: _ }| (target as _, at as _))
        .map_err(|err| eyre!("Sync failed because: {err}"));

    if let Some(syncing_result) = syncing_progress.next().await {
        let (target_block, current_block) = syncing_result?;
        let syncing_progress_bar = syncing_progress_bar(current_block, target_block);

        while let Some(stream_result) = syncing_progress.next().await {
            let (target_block, current_block) = stream_result?;
            syncing_progress_bar.set_position(current_block);
            syncing_progress_bar.set_length(target_block);
        }

        syncing_progress_bar.finish_with_message("Syncing is done!");
    }
    Ok(())
}

async fn subscribe_to_plotting_progress(
    summary: Summary,
    farmer: Farmer,
    is_initial_progress_finished: Arc<AtomicBool>,
    sector_size_bytes: u64,
) {
    tokio::spawn({
        let summary = summary.clone();
        async move {
            for (plot_id, plot) in farmer.iter_plots().await.enumerate() {
                println!("Initial plotting for plot: #{plot_id} ({})", plot.directory().display());
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
                    .expect("hardcoded template is correct")
                    .progress_chars("=> "),
                );
                progress_bar.finish_with_message("Initial plotting finished!");
                is_initial_progress_finished.store(true, Ordering::Relaxed);
                let _ = summary.update(Some(true), None).await; // ignore the
                                                                // error, since
                                                                // we will abandon
                                                                // this mechanism
                                                                // soon
            }
        }
    });
}

async fn subscribe_to_solutions(
    summary: Summary,
    farmer: Farmer,
    is_initial_progress_finished: Arc<AtomicBool>,
) {
    tokio::spawn({
        async move {
            let farmed_blocks = summary
                .get_farmed_block_count()
                .await
                .expect("couldn't read farmed blocks count from summary");
            let farmed_block_count = Arc::new(AtomicU64::new(farmed_blocks));
            for plot in farmer.iter_plots().await {
                plot.subscribe_new_solutions()
                    .await
                    .for_each(|solutions| {
                        let farmed_block_count = &farmed_block_count;
                        let is_initial_progress_finished = &is_initial_progress_finished;
                        let summary = summary.clone();
                        async move {
                            if !solutions.solutions.is_empty() {
                                let total_farmed =
                                    farmed_block_count.fetch_add(1, Ordering::Relaxed);
                                let _ = summary.update(None, Some(total_farmed)).await; // ignore the error, since we will abandon this mechanism
                                if is_initial_progress_finished.load(Ordering::Relaxed) {
                                    println!("You have farmed {total_farmed} block(s) in total!");
                                }
                            }
                        }
                    })
                    .await
            }
        }
    });
}

/// nice looking progress bar for the initial plotting :)
fn plotting_progress_bar(total_size: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_size);
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb.set_style(
        ProgressStyle::with_template(
            " {spinner:2.green} [{elapsed_precise}] {percent}% [{wide_bar:.yellow}] ({pos}/{len}) \
             {bytes_per_sec}, {msg}, ETA: {eta_precise} ",
        )
        .expect("hardcoded template is correct")
        // More of those: https://github.com/sindresorhus/cli-spinners/blob/45cef9dff64ac5e36b46a194c68bccba448899ac/spinners.json
        .tick_strings(&["◜", "◠", "◝", "◞", "◡", "◟"])
        // From here: https://github.com/console-rs/indicatif/blob/d54fb0ef4c314b3c73fc94372a97f14c4bd32d9e/examples/finebars.rs#L10
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message("plotting");
    pb
}

/// nice looking progress bar for the syncing :)
fn syncing_progress_bar(current_block: u64, total_blocks: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_blocks);
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb.set_style(
        ProgressStyle::with_template(
            " {spinner:2.green} [{elapsed_precise}] {percent}% [{wide_bar:.cyan}] ({pos}/{len}) \
             {bps}, {msg}, ETA: {eta_precise} ",
        )
        .expect("hardcoded template is correct")
        .with_key("bps", |state: &indicatif::ProgressState, w: &mut dyn std::fmt::Write| {
            write!(w, "{:.2}bps", state.per_sec()).expect("terminal write should succeed")
        })
        // More of those: https://github.com/sindresorhus/cli-spinners/blob/45cef9dff64ac5e36b46a194c68bccba448899ac/spinners.json
        .tick_strings(&["◜", "◠", "◝", "◞", "◡", "◟"])
        // From here: https://github.com/console-rs/indicatif/blob/d54fb0ef4c314b3c73fc94372a97f14c4bd32d9e/examples/finebars.rs#L10
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message("syncing");
    pb.set_position(current_block);
    pb
}
