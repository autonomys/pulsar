use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use color_eyre::eyre::{eyre, Context, Result};
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
use crate::summary::{Rewards, Summary, SummaryFile, SummaryUpdateFields};
use crate::utils::{install_tracing, raise_fd_limit, spawn_task, IntoEyre, IntoEyreStream};

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
            node.sync().await.into_eyre().context("Node syncing failed")?;
        }
    }

    let summary_file = SummaryFile::new(Some(farmer_config.plot_size)).await?;

    println!("Starting farmer ...");
    let farmer = Arc::new(farmer_config.build(&node).await?);
    println!("Farmer started successfully!");

    let maybe_handles = if !is_verbose {
        // this will be shared between the two subscriptions
        let is_initial_progress_finished = Arc::new(AtomicBool::new(false));
        let sector_size_bytes =
            farmer.get_info().await.into_eyre().context("Failed to get farmer into")?.sector_size;

        let plotting_sub_handle = spawn_task(
            "plotting_subscriber",
            subscribe_to_plotting_progress(
                summary_file.clone(),
                farmer.clone(),
                is_initial_progress_finished.clone(),
                sector_size_bytes,
            ),
        );

        let solution_sub_handle = spawn_task(
            "solution_subscriber",
            subscribe_to_solutions(
                summary_file.clone(),
                node.clone(),
                is_initial_progress_finished.clone(),
                reward_address,
            ),
        );

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
    if let Some((plotting_handle, solution_handle)) = maybe_handles.as_mut() {
        futures::select! {
            _ = signal::ctrl_c().fuse() => {
               println!(
                    "\nWill try to gracefully exit the application now. Please wait for a couple of seconds... If you press ctrl+c again, it will \
                    try to forcefully close the app!"
                );
                plotting_handle.abort();
                solution_handle.abort();
            }
            res = solution_handle.fuse() => {
                return res.context("couldn't join subscription handle")?.context("solution subscription crashed");
            }
            // cannot inspect plotting sub for errors, since it may end and quit from select
        }
    } else {
        // if there are not subscriptions, just wait on the kill signal
        signal::ctrl_c().await?;
    }

    // shutting down the farmer and the node
    let graceful_close_handle = spawn_task("graceful_shutdown_listener", async move {
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
        .into_eyre()
        .context("Failed to subscribe to node syncing")?
        .into_eyre()
        .map_ok(|SyncingProgress { at, target, status: _ }| (target as _, at as _));

    if let Some(syncing_result) = syncing_progress.next().await {
        let (target_block, current_block) = syncing_result.context("Sync failed")?;
        let syncing_progress_bar = syncing_progress_bar(current_block, target_block);

        while let Some(stream_result) = syncing_progress.next().await {
            let (target_block, current_block) = stream_result.context("Sync failed")?;
            syncing_progress_bar.set_position(current_block);
            syncing_progress_bar.set_length(target_block);
        }
        syncing_progress_bar.finish_with_message(
            "Initial syncing is completed! Syncing will continue in the background...",
        );
    }
    Ok(())
}

async fn subscribe_to_plotting_progress(
    summary_file: SummaryFile,
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
    summary_file
        .update(SummaryUpdateFields { is_plotting_finished: true, ..Default::default() })
        .await
        .context("couldn't update the summary")?;

    Ok(())
}

async fn subscribe_to_solutions(
    summary_file: SummaryFile,
    node: Arc<Node>,
    is_initial_progress_finished: Arc<AtomicBool>,
    reward_address: PublicKey,
) -> Result<()> {
    // necessary for spacing
    println!();

    let Summary { last_processed_block_num: last_block_num, .. } = summary_file.parse().await?;

    let stream = not_yet_processed_block_nums_stream(node.clone(), last_block_num);

    futures::pin_mut!(stream);

    // to update the last scanned block, we keep a counter
    let mut current_iter = 0;
    let n_blocks = 1000;
    let n_tasks = 10;

    stream
        // Map block number to block hash
        .and_then(|block| {
            node.block_hash(block)
                .transpose()
                .map(futures::future::ready)
                .expect("TODO: Account for missing blocks somehow or check pruning")
        })
        // Chunk block hashes in chunks of `n_blocks`
        .try_chunks(n_blocks)
        .map_err(anyhow::Error::from)
        // For each n_blocks
        .try_for_each(|blocks| {
            let node_clone = node.clone();
            let summary_clone = summary_file.clone();
            async move {
                current_iter += 1;
                // We iterate over hashes
                let (rewards, votes, author) = futures::stream::iter(blocks)
                    // We scan each hash and find 3 things:
                    // - Total amount of rewards
                    // - Number of votes
                    // - Number of times we authored a block
                    .map(|hash| {
                        // Auth
                        let is_author = node_clone
                            .block_header(hash)?
                            .expect("TODO: Account for missing blocks somehow or check pruning")
                            .pre_digest
                            .map(|pre_digest| pre_digest.solution.reward_address == reward_address)
                            .unwrap_or_default();

                        let rewards_future = node_clone
                            .get_events(Some(hash))
                            .map_ok(|events| {
                                events
                                    .into_iter()
                                    .map(|event| match event {
                                        Event::Rewards(
                                            RewardsEvent::VoteReward { voter: author, reward }
                                            | RewardsEvent::BlockReward {
                                                block_author: author,
                                                reward,
                                            },
                                        ) if author == reward_address.into() => (reward, 0),
                                        Event::Subspace(SubspaceEvent::FarmerVote {
                                            reward_address: author,
                                            ..
                                        }) if author == reward_address.into() => (0, 1),
                                        _ => (0, 0),
                                    })
                                    .fold((0, 0), |(rewards, votes), (new_rewards, new_votes)| {
                                        (rewards + new_rewards, votes + new_votes)
                                    })
                            })
                            .map_ok(move |(rewards, votes)| {
                                (rewards, votes, if is_author { 1 } else { 0 })
                            });

                        Ok(rewards_future)
                    })
                    // We calculate each block in parallel
                    .try_buffer_unordered(n_tasks)
                    // After that we sum up result
                    .try_fold(
                        (0, 0, 0),
                        |(rewards, votes, author), (new_rewards, new_votes, new_author)| {
                            futures::future::ok((
                                rewards + new_rewards,
                                votes + new_votes,
                                author + new_author,
                            ))
                        },
                    )
                    .await?;

                summary_clone
                    .update(SummaryUpdateFields {
                        maybe_authored_count: Some(author),
                        maybe_vote_count: Some(votes),
                        maybe_reward: Some(Rewards(rewards)),
                        maybe_new_blocks: Some(current_iter * 1000),
                        ..Default::default()
                    })
                    .await
                    .expect("opsie");

                Ok(())
            }
        })
        .await
        .into_eyre()?;

    let Summary { total_rewards, authored_count, vote_count, last_processed_block_num, .. } =
        summary_file.parse().await.context("couldn't update summary")?;

    if is_initial_progress_finished.load(Ordering::Relaxed) {
        print!(
            "\rYou have earned: {total_rewards} SSC(s), farmed {authored_count} block(s), and \
             voted on {vote_count} block(s)! This data is derived from the first \
             {last_processed_block_num} blocks.\n",
        );
        // use carriage return to overwrite the current value
        // instead of inserting a new line
        std::io::stdout().flush().expect("Failed to flush stdout");
        // flush the stdout to make sure values are printed
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

fn not_yet_processed_block_nums_stream(
    node: std::sync::Arc<Node>,
    mut last_block_num: subspace_sdk::node::BlockNumber,
) -> impl Stream<Item = anyhow::Result<subspace_sdk::node::BlockNumber>> {
    async_stream::try_stream! {
        loop {
            let last_retrieved_num = node.get_info().await?.finalized_block.1;

            if last_block_num == last_retrieved_num {
                break;
            }

            while last_block_num < last_retrieved_num {
                yield last_block_num;
                last_block_num += 1;
            }
        }
    }
}
