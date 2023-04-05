use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use color_eyre::eyre::{eyre, Context, Report, Result};
use futures::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use single_instance::SingleInstance;
use subspace_sdk::node::{Event, RewardsEvent, SubspaceEvent, SyncingProgress};
use subspace_sdk::{Farmer, Node, PublicKey};
use tokio::signal;
use tokio::task::JoinHandle;
use tracing::instrument;

use crate::config::{validate_config, ChainConfig, Config};
use crate::summary::{Summary, SummaryUpdateFields};
use crate::utils::{install_tracing, raise_fd_limit};

/// allows us to detect multiple instances of the farmer and act on it
pub(crate) const SINGLE_INSTANCE: &str = ".subspaceFarmer";
type MaybeHandles = Option<(JoinHandle<Result<()>>, JoinHandle<Result<()>>)>;

/// implementation of the `farm` command
///
/// takes `is_verbose`, returns a [`Farmer`], [`Node`], and a [`SingleInstance`]
///
/// first, checks for an existing farmer instance
/// then starts the farming and node instances,
/// lastly, depending on the verbosity, it subscribes to plotting progress and
/// new solutions
#[instrument]
pub(crate) async fn farm(is_verbose: bool, executor: bool) -> Result<()> {
    install_tracing(is_verbose);
    color_eyre::install()?;

    let instance = SingleInstance::new(SINGLE_INSTANCE)
        .context("Cannot take the instance lock from the OS! Aborting...")?;
    if !instance.is_single() {
        return Err(eyre!(
            "It seems like there is already a farming instance running. Aborting...",
        ));
    }
    // raise file limit
    raise_fd_limit();

    let Config { chain, farmer: farmer_config, node: mut node_config } = validate_config()?;
    let reward_address = farmer_config.reward_address;

    // apply advanced options (flags)
    if executor {
        println!("Setting the {} flag for the node...", "executor".underline());
        node_config.advanced.executor = true;
    }

    println!("Starting node ...");
    let node = Arc::new(
        node_config.build(chain.clone(), is_verbose).await.context("error building the node")?,
    );
    println!("Node started successfully!");

    if !matches!(chain, ChainConfig::Dev) {
        if !is_verbose {
            subscribe_to_node_syncing(&node).await?;
        } else {
            node.sync().await.map_err(|err| eyre!("Node syncing failed: {err}"))?;
        }
    }

    let summary = Summary::new(Some(farmer_config.plot_size)).await?;

    println!("Starting farmer ...");
    let farmer = Arc::new(farmer_config.build(&node).await?);
    println!("Farmer started successfully!");

    let maybe_handles = if !is_verbose {
        // this will be shared between the two subscriptions
        let is_initial_progress_finished = Arc::new(AtomicBool::new(false));
        let sector_size_bytes = farmer.get_info().await.map_err(Report::msg)?.sector_size;

        let plotting_sub_handle = tokio::spawn(subscribe_to_plotting_progress(
            summary.clone(),
            farmer.clone(),
            is_initial_progress_finished.clone(),
            sector_size_bytes,
        ));

        let solution_sub_handle = tokio::spawn(subscribe_to_solutions(
            summary.clone(),
            node.clone(),
            is_initial_progress_finished.clone(),
            reward_address,
        ));

        Some((plotting_sub_handle, solution_sub_handle))
    } else {
        // we don't have handles if it is verbose
        None
    };

    wait_on_farmer(maybe_handles, farmer, node).await?;

    Ok(())
}

#[instrument]
async fn wait_on_farmer(
    mut maybe_handles: MaybeHandles,
    farmer: Arc<Farmer>,
    node: Arc<Node>,
) -> Result<()> {
    // node subscription can be gracefully closed with `ctrl_c` without any problem
    // (no code needed). We need graceful closing for farmer subscriptions.
    if let Some((_plotting_handle, solution_handle)) = maybe_handles.as_mut() {
        tokio::select! {
            _ = signal::ctrl_c() => {
               println!(
                    "\nWill try to gracefully exit the application now. Please wait for a couple of seconds... If you press ctrl+c again, it will \
                    try to forcefully close the app!"
                );
                let (plotting_handle, solution_handle) = maybe_handles.as_ref().expect("check is up there");
                plotting_handle.abort();
                solution_handle.abort();
            }
            res = solution_handle => {
                // first level is join handle, second layer is actual result from the task
                match res {
                    Ok(Ok(())) => unreachable!("solution subscription never ends"),
                    Ok(Err(err)) => return Err(eyre!("solution subscription crashed with err: {err}")),
                    Err(err) => return Err(eyre!("couldn't join subscription handle with err: {err:?}"))
                }
            }
            // cannot inspect plotting subscription for errors, since it may end and quit from
            // `tokio::select`
        }
    } else {
        // if there are not subscriptions, just wait on the kill signal
        signal::ctrl_c().await?;
    }

    // shutting down the farmer and the node
    let graceful_close_handle = tokio::spawn(async move {
        // if one of the subscriptions have not aborted yet, wait
        // Plotting might end, so we ignore result here

        if let Some((plotting_handle, solution_handle)) = maybe_handles {
            let _ = plotting_handle.await;
            solution_handle.await.expect_err("Solution subscription never ends");
        }

        Arc::try_unwrap(farmer)
            .expect("there should have been only 1 strong farmer counter")
            .close()
            .await
            .expect("cannot close farmer");
        Arc::try_unwrap(node)
            .expect("there should have been only 1 strong node counter")
            .close()
            .await
            .expect("cannot close node");
    });

    tokio::select! {
        _ = graceful_close_handle => println!("gracefully closed the app!"),
        _ = signal::ctrl_c() => println!("\nforcefully closing the app!"),
    }
    Ok(())
}

#[instrument]
async fn subscribe_to_node_syncing(node: &Node) -> Result<()> {
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
        syncing_progress_bar.finish_with_message(
            "Initial syncing is completed! Syncing will continue in the background...",
        );
    }
    Ok(())
}

#[instrument]
async fn subscribe_to_plotting_progress(
    summary: Summary,
    farmer: Arc<Farmer>,
    is_initial_progress_finished: Arc<AtomicBool>,
    sector_size_bytes: u64,
) -> Result<()> {
    for (plot_id, plot) in farmer.iter_plots().await.enumerate() {
        println!("Initial plotting for plot: #{plot_id} ({})", plot.directory().display());

        let mut plotting_progress = plot.subscribe_initial_plotting_progress().await;
        let progress_bar;

        if let Some(plotting_result) = plotting_progress.next().await {
            let current_size = plotting_result.current_sector * sector_size_bytes;
            progress_bar = plotting_progress_bar(current_size, plot.allocated_space().as_u64());

            while let Some(stream_result) = plotting_progress.next().await {
                let current_size = stream_result.current_sector * sector_size_bytes;
                progress_bar.set_position(current_size);
            }
        } else {
            // means initial plotting was already finished
            progress_bar = plotting_progress_bar(
                plot.allocated_space().as_u64(),
                plot.allocated_space().as_u64(),
            );
        }
        progress_bar.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] {percent}% [{bar:40.green/blue}] ({bytes}/{total_bytes}) \
                 {msg}",
            )
            .expect("hardcoded template is correct"),
        );
        progress_bar.finish_with_message("Initial plotting finished!\n");
    }
    is_initial_progress_finished.store(true, Ordering::Relaxed);
    summary
        .update(SummaryUpdateFields { is_plotting_finished: true, ..Default::default() })
        .await
        .context("couldn't update the summary")?;

    Ok(())
}

#[instrument]
async fn subscribe_to_solutions(
    summary: Summary,
    node: Arc<Node>,
    is_initial_progress_finished: Arc<AtomicBool>,
    reward_address: PublicKey,
) -> Result<()> {
    println!();
    let mut new_blocks = node
        .subscribe_new_blocks()
        .await
        .map_err(|err| eyre!("couldn't subscribe to new blocks, because: {err}"))?;
    while let Some(new_block) = new_blocks.next().await {
        let mut summary_update_values: SummaryUpdateFields = Default::default();

        let events = node
            .get_events(Some(new_block.hash))
            .await
            .map_err(|err| eyre!("couldn't get events from node, because: {err}"))?;

        for event in events {
            // subscription is active when plotting is started, only print out rewards after
            // plotting finishes to not corrup the progress bars
            if is_initial_progress_finished.load(Ordering::Relaxed) {
                match event {
                    Event::Rewards(
                        RewardsEvent::VoteReward { voter: author, reward }
                        | RewardsEvent::BlockReward { block_author: author, reward },
                    ) if author == reward_address.into() =>
                        summary_update_values.maybe_new_reward = Some(reward),
                    Event::Subspace(SubspaceEvent::FarmerVote {
                        reward_address: author, ..
                    }) if author == reward_address.into() =>
                        summary_update_values.is_new_vote = true,
                    _ => (),
                }
            }
        }

        if let Some(pre_digest) = new_block.pre_digest {
            if pre_digest.solution.reward_address == reward_address {
                summary_update_values.is_new_block_farmed = true;
            }
        }
        let _ = summary.update(summary_update_values).await; // ignore the
                                                             // error, we will
                                                             // abandon this
                                                             // mechanism
        let (farmed_block_count, vote_count, total_rewards) = (
            summary
                .get_farmed_block_count()
                .await
                .context("couldn't read farmed block count value, summary was corrupted")?,
            summary
                .get_vote_count()
                .await
                .context("couldn't read vote count value, summary was corrupted")?,
            summary
                .get_total_rewards()
                .await
                .context("couldn't read total rewards value, summary was corrupted")?,
        );

        if is_initial_progress_finished.load(Ordering::Relaxed) {
            print!(
                "\rYou have earned: {total_rewards} SSC(s), farmed {farmed_block_count} block(s), \
                 and voted on {vote_count} block(s)!"
            );
            // use
            // carriage return to overwrite the current value
            // instead of inserting
            // a new line
            std::io::stdout().flush().expect("Failed to flush stdout");
            // flush the
            // stdout to make sure values are printed
        }
    }
    Ok(())
}

/// nice looking progress bar for the initial plotting :)
fn plotting_progress_bar(current_size: u64, total_size: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_size);
    // pb.enable_steady_tick(std::time::Duration::from_millis(100)); // TODO:
    // uncomment this when plotting is considerably faster
    pb.set_style(
        ProgressStyle::with_template(
            " {spinner:2.green} [{elapsed_precise}] {percent}% [{wide_bar:.orange}] \
             ({bytes}/{total_bytes}) {bytes_per_sec}, {msg}, ETA: {eta_precise} ",
        )
        .expect("hardcoded template is correct")
        // More of those: https://github.com/sindresorhus/cli-spinners/blob/45cef9dff64ac5e36b46a194c68bccba448899ac/spinners.json
        .tick_strings(&["◜", "◠", "◝", "◞", "◡", "◟"])
        // From here: https://github.com/console-rs/indicatif/blob/d54fb0ef4c314b3c73fc94372a97f14c4bd32d9e/examples/finebars.rs#L10
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message("plotting");
    pb.set_position(current_size);
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
