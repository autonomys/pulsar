use std::{thread, time::Duration};

use color_eyre::eyre::Result;
use futures::prelude::*;
use indicatif::{ProgressBar, ProgressStyle};
use subspace_sdk::Farmer;
use subspace_sdk::{chain_spec, Node, PlotDescription, PublicKey};

use tracing::instrument;

use crate::config::parse_config;
use crate::utils::{install_tracing, node_directory_getter};

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
    tokio::spawn(async move {
        // farmer.iter_plots().await.for_each(|plot| async move {
        //     plot.subscribe_plotting_progress()
        //         .await
        //         .for_each(|progress| async move {
        //             println!(
        //                 "Progress is: {:?} out of {:?}",
        //                 progress.current_sector, progress.total_sectors
        //             )
        //         })
        //         .await;
        // })
        for plot in farmer.iter_plots().await {
            plot.subscribe_plotting_progress()
                .await
                .for_each(|progress| async move {
                    println!(
                        "Progress is: {:?} out of {:?}",
                        progress.current_sector, progress.total_sectors
                    )
                })
                .await
        }
    });

    println!("sleeping for now");
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    println!("awoken!");

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

// TODO: have a callback here for updating the value of currently encoded
fn _plotting_progress_bar(total_size: u64) {
    let mut encoded = 0;
    let to_be_plotted = total_size;

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner} [{elapsed_precise}] [{wide_bar}]
     {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    while encoded < to_be_plotted {
        encoded += 123123;
        pb.set_position(encoded);
        thread::sleep(Duration::from_millis(12));
    }

    pb.finish_with_message("downloaded");
}
