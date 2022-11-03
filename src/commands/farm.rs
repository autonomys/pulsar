use color_eyre::eyre::Result;
use color_eyre::eyre::WrapErr;
use subspace_sdk::Farmer;
use subspace_sdk::{chain_spec, Node, PlotDescription, PublicKey};
use tracing::instrument;

use crate::config::parse_config;
use crate::utils::{install_tracing, node_directory_getter};

pub(crate) struct FarmingArgs {
    reward_address: PublicKey,
    node: Node,
    plot: PlotDescription,
}

pub(crate) async fn farm(is_verbose: bool) -> Result<()> {
    install_tracing(is_verbose);
    color_eyre::install()?;
    let args = prepare_farming().await?;
    start_farming(args).await?;

    Ok(())
}

#[instrument(skip(farming_args))]
async fn start_farming(farming_args: FarmingArgs) -> Result<Farmer> {
    let FarmingArgs {
        reward_address,
        node,
        plot,
    } = farming_args;

    Farmer::builder()
        .build(reward_address, node, &[plot])
        .await
        .wrap_err("error building the farmer")
}

#[instrument]
async fn prepare_farming() -> Result<FarmingArgs> {
    let config_args = parse_config()?;

    let chain = config_args.node_config_args.chain;
    let node_name = config_args.node_config_args.name;
    let node_directory = node_directory_getter();
    let chain = match chain.as_str() {
        "gemini-2a" => chain_spec::gemini_2a().unwrap(),
        "dev" => chain_spec::dev_config().unwrap(),
        _ => unreachable!("there are no other valid chain-specs at the moment"),
    };

    let node: Node = Node::builder()
        .name(node_name)
        .build(node_directory, chain)
        .await
        .expect("error building the node");

    Ok(FarmingArgs {
        reward_address: config_args.farmer_config_args.reward_address,
        plot: config_args.farmer_config_args.plot,
        node,
    })
}
