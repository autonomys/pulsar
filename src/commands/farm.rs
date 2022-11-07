use color_eyre::eyre::Result;
use futures::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use subspace_sdk::Farmer;
use subspace_sdk::{chain_spec, Node, PlotDescription, PublicKey};

use tracing::instrument;

use crate::config::parse_config;
use crate::utils::{install_tracing, node_directory_getter};

// TODO: if there is a way to get this from the SDK, revamp this
const SECTOR_SIZE: u64 = 2750000;

#[derive(Debug)]
pub(crate) struct FarmingArgs {
    reward_address: PublicKey,
    node: Node,
    plot: PlotDescription,
}

#[instrument]
pub(crate) async fn farm(is_verbose: bool) -> Result<()> {
    install_tracing(is_verbose);
    color_eyre::install()?;
    let args = prepare_farming().await?;
    let (mut farmer, _node) = start_farming(args).await?;

    if !is_verbose {
        tokio::spawn(async move {
            for (plot_id, plot) in farmer.iter_plots().await.enumerate() {
                println!(
                    "Initial plotting for plot: #{plot_id} ({})",
                    plot.directory().display()
                );
                let progress_bar = plotting_progress_bar(plot.allocated_space().as_u64());
                plot.subscribe_plotting_progress()
                    .await
                    .for_each(|progress| {
                        let pb_clone = progress_bar.clone();
                        async move {
                            let current_bytes = progress.current_sector * SECTOR_SIZE;
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
            }
        });
    }

    Ok(())
}

#[instrument]
async fn start_farming(farming_args: FarmingArgs) -> Result<(Farmer, Node)> {
    let FarmingArgs {
        reward_address,
        node,
        plot,
    } = farming_args;

    Ok((
        Farmer::builder()
            .build(reward_address, node.clone(), &[plot])
            .await?,
        node,
    ))
}

#[instrument]
async fn prepare_farming() -> Result<FarmingArgs> {
    let config_args = parse_config()?;

    let node_name = config_args.node_config_args.name;
    let chain = match config_args.node_config_args.chain.as_str() {
        "gemini-2a" => chain_spec::gemini_2a().unwrap(),
        "dev" => chain_spec::dev_config().unwrap(),
        _ => unreachable!("there are no other valid chain-specs at the moment"),
    };
    let role = match config_args.node_config_args.validator {
        true => subspace_sdk::node::Role::Authority,
        false => subspace_sdk::node::Role::Full,
    };
    let node_directory = node_directory_getter();

    let node: Node = Node::builder()
        .name(node_name)
        .role(role)
        .build(node_directory, chain)
        .await
        .expect("error building the node");

    Ok(FarmingArgs {
        reward_address: config_args.farmer_config_args.reward_address,
        plot: config_args.farmer_config_args.plot,
        node,
    })
}

fn plotting_progress_bar(total_size: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner} [{elapsed_precise}] {percent}% [{bar:40.cyan/blue}]
      ({bytes}/{total_bytes}) {bytes_per_sec}, {msg}, remaining time: {eta}",
        )
        .unwrap()
        .progress_chars("=> "),
    );
    pb.set_message("plotting");

    pb
}
