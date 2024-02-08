use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use color_eyre::eyre::{bail, eyre, Context, Error, Result};
use color_eyre::Report;
use futures::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use single_instance::SingleInstance;
use subspace_sdk::node::{Hash, SyncingProgress};
use subspace_sdk::{Farmer, Node, PublicKey};
use tokio::signal;
use tokio::task::JoinHandle;
use tracing::instrument;

use crate::config::{validate_config, ChainConfig, Config};
use crate::summary::{Summary, SummaryFile, SummaryUpdateFields};
use crate::utils::{install_tracing, raise_fd_limit, spawn_task, IntoEyre, IntoEyreStream};

/// allows us to detect multiple instances of the farmer and act on it
pub(crate) const SINGLE_INSTANCE: &str = ".subspaceFarmer";
const BATCH_BLOCKS: usize = 1000;
const N_TASKS: usize = 10;

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
pub(crate) async fn farm(is_verbose: bool, enable_domains: bool, no_rotation: bool) -> Result<()> {
    install_tracing(is_verbose, no_rotation);
    color_eyre::install()
        .context("color eyre installment failed, it should have been the first one")?;

    let instance = SingleInstance::new(SINGLE_INSTANCE)
        .context("Cannot take the instance lock from the OS! Aborting...")?;
    if !instance.is_single() {
        bail!("It seems like there is already a farming instance running. Aborting...",)
    }
    // raise file limit
    raise_fd_limit();

    let Config { chain, farmer: farmer_config, node: mut node_config } =
        validate_config().context("couldn't validate config")?;
    let reward_address = farmer_config.reward_address;

    // apply advanced options (flags)
    if enable_domains {
        println!("Setting the {} flag for the node...", "enable_domains".underline());
        node_config.advanced.enable_domains = true;
    }

    println!("Starting node ...");
    let node = Arc::new(
        node_config
            .clone()
            .build(chain.clone(), is_verbose)
            .await
            .context("error building the node")?,
    );
    println!("Node started successfully!");

    if !matches!(chain, ChainConfig::Dev) {
        if !is_verbose {
            subscribe_to_node_syncing(&node).await.context("couldn't subscribe to syncing")?;
        } else {
            node.sync().await.into_eyre().context("Node syncing failed")?;
        }
    }

    let summary_file = SummaryFile::new(Some(farmer_config.farm_size))
        .await
        .context("constructing new SummaryFile failed")?;

    println!("Starting farmer ...");
    let farmer = Arc::new(farmer_config.build(&node).await.context("farmer couldn't be build")?);
    println!("Farmer started successfully!");

    let maybe_handles = if !is_verbose {
        // we need this to handle errors when block is not found
        // if this fails, it might be due to: https://github.com/toml-rs/toml/issues/405 and https://github.com/toml-rs/toml/issues/329
        let blocks_pruning =
            node_config.advanced.clone().extra.get("blocks_pruning").is_some_and(|value| {
                value.as_table().is_some_and(|table| {
                    table.get("Some").is_some_and(|value| value.as_integer().is_some())
                })
            });

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
                blocks_pruning,
            ),
        );

        Some((plotting_sub_handle, solution_sub_handle))
    } else {
        // we don't have handles if it is verbose
        None
    };

    wait_on_farmer(maybe_handles, farmer, node).await.context("waiting on farmer failed")?;

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
        signal::ctrl_c().await.context("failed to listen ctrl-c event")?
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
    for (farm_id, farm) in farmer.iter_farms().await.enumerate() {
        println!("Initial plotting for farm: #{farm_id} ({})", farm.directory().display());

        let mut plotting_progress = farm.subscribe_initial_plotting_progress().await;
        let progress_bar;

        if let Some(plotting_result) = plotting_progress.next().await {
            let current_size = plotting_result.current_sector * sector_size_bytes;
            progress_bar = plotting_progress_bar(current_size, farm.allocated_space().as_u64());

            while let Some(stream_result) = plotting_progress.next().await {
                let current_size = stream_result.current_sector * sector_size_bytes;
                progress_bar.set_position(current_size);
            }
        } else {
            // means initial plotting was already finished
            progress_bar = plotting_progress_bar(
                farm.allocated_space().as_u64(),
                farm.allocated_space().as_u64(),
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
    blocks_pruning: bool,
) -> Result<()> {
    // necessary for spacing
    println!();

    let Summary { last_processed_block_num, .. } =
        summary_file.parse().await.context("parsing the summary failed")?;

    // first, process the stream in a parallelized fashion,
    // after that, there will be new blocks arrived
    // process these new blocks sequentially
    process_block_stream(
        last_processed_block_num,
        node.clone(),
        blocks_pruning,
        summary_file.clone(),
        reward_address,
        BATCH_BLOCKS,
        N_TASKS,
    )
    .await
    .context("parallel block stream couldn't be processed")?;

    loop {
        let Summary { authored_count, last_processed_block_num, .. } =
            summary_file.parse().await.context("couldn't parse summary")?;

        if is_initial_progress_finished.load(Ordering::Relaxed) {
            // use carriage return to overwrite the current value
            // instead of inserting a new line
            print!(
                "\rYou have farmed {authored_count} block(s), This data is derived from the first \
                 {last_processed_block_num} blocks.",
            );
            // flush the stdout to make sure values are printed
            std::io::stdout().flush().expect("Failed to flush stdout");

            // now, process the blocks without paralellization
            process_block_stream(
                last_processed_block_num,
                node.clone(),
                blocks_pruning,
                summary_file.clone(),
                reward_address,
                1,
                1,
            )
            .await
            .context("sequential block stream couldn't be processed")?;
        }
        // sleep 2 secs to avoid spamming the print
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

fn not_yet_processed_block_nums_stream(
    node: std::sync::Arc<Node>,
    mut last_processed_block_num: subspace_sdk::node::BlockNumber,
) -> impl Stream<Item = Result<subspace_sdk::node::BlockNumber>> {
    async_stream::try_stream! {
        loop {
            let last_retrieved_block_num = node.get_info().await.into_eyre().context("failed to receive Info from node")?.best_block.1;

            if last_processed_block_num > last_retrieved_block_num {
                Err(eyre!("last_processed_block_num is greater than last_retrieved_block_num")).context("Try wiping the summary file and restart")?;
            }

            if last_processed_block_num == last_retrieved_block_num {
                break;
            }

            while last_processed_block_num < last_retrieved_block_num {
                last_processed_block_num += 1;
                yield last_processed_block_num;

            }
        }
    }
}

async fn process_block_stream(
    last_processed_block_num: subspace_sdk::node::BlockNumber,
    node: Arc<Node>,
    blocks_pruning: bool,
    summary_file: SummaryFile,
    reward_address: PublicKey,
    batch_blocks: usize,
    n_tasks: usize,
) -> Result<()> {
    let stream = not_yet_processed_block_nums_stream(node.clone(), last_processed_block_num);

    futures::pin_mut!(stream);

    stream
        .try_filter_map(|block| match node.block_hash(block).into_eyre() {
            Ok(Some(block_hash)) => future::ok(Some(block_hash)),
            Ok(None) if blocks_pruning => future::ok(None),
            Ok(None) =>
                future::err(eyre!("node database is probably corrupted, try wiping the node")),
            Err(err) => future::err(err.wrap_err("couldn't get block hash from node")),
        })
        // Chunk block hashes in chunks of `n_blocks`
        .try_chunks(batch_blocks)
        .map_err(|err| Error::from(err).wrap_err("Fetching blocks failed"))
        // For each n_blocks
        .try_for_each(|blocks| {
            let node_clone = node.clone();
            let summary_clone = summary_file.clone();
            async move {
                let block_count = blocks.len() as u32;
                // We iterate over hashes
                let author = get_author_info_from_blocks(
                    node_clone,
                    blocks,
                    reward_address,
                    n_tasks,
                    blocks_pruning,
                )
                .await
                .context("couldn't get author info")?;

                summary_clone
                    .update(SummaryUpdateFields {
                        new_authored_count: author,
                        new_parsed_blocks: block_count,
                        ..Default::default()
                    })
                    .await
                    .context("couldn't update the summary")?;

                Ok(())
            }
        })
        .await
}

async fn get_author_info_from_blocks(
    node: Arc<Node>,
    blocks: Vec<Hash>,
    reward_address: PublicKey,
    n_tasks: usize,
    blocks_pruning: bool,
) -> Result<u64> {
    let author = futures::stream::iter(blocks)
        // We scan each hash and find 3 things:
        // - Total amount of rewards
        // - Number of votes
        // - Number of times we authored a block
        .map(|hash| {
            let is_author = match node
                .block_header(hash)
                .into_eyre()
                .context("failed to retrieve block header from node")?
            {
                Some(block_header) => Ok(block_header
                    .pre_digest
                    .map(|pre_digest| pre_digest.solution().reward_address == reward_address)
                    .unwrap_or_default()),
                None if blocks_pruning => Ok(false),
                None => Err(eyre!("node database is probably corrupted, try wiping the node")),
            }
            .context("couldn't get the author info from block header")?;

            Result::Ok(futures::future::ok::<u64, Report>(if is_author { 1 } else { 0 }))
        })
        // We calculate each block in parallel
        .try_buffer_unordered(n_tasks)
        // After that we sum up result
        .try_fold(0, |author, new_author| futures::future::ok(author + new_author))
        .await
        .context("error in stream encountered in try_fold step")?;

    Ok(author)
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
