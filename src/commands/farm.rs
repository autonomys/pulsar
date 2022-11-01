use subspace_sdk::{chain_spec, Node, PlotDescription, PublicKey};
use subspace_sdk::{Farmer, NodeMode};

use crate::config::{parse_config, ConfigParseError};
use crate::utils::node_directory_getter;

pub(crate) struct FarmingArgs {
    reward_address: PublicKey,
    node: Node,
    plot: PlotDescription,
}

pub(crate) async fn farm() {
    match prepare_farming().await {
        Ok(args) => start_farming(args).await,
        Err(why) => panic!("Error: {why}"),
    }
}

async fn start_farming(farming_args: FarmingArgs) {
    let FarmingArgs {
        reward_address,
        node,
        plot,
    } = farming_args;

    let _ = Farmer::builder()
        .build(reward_address, node, &[plot])
        .await
        .expect("farmer builder failed");
}

async fn prepare_farming() -> Result<FarmingArgs, ConfigParseError> {
    let config_args = parse_config()?;

    // TODO: use the below when SDK is compatible with it
    // let chain = config_args.node_config_args.chain;
    let node_name = config_args.node_config_args.name;
    let node_directory = node_directory_getter();
    let node: Node = Node::builder()
        .mode(NodeMode::Full)
        .name(node_name)
        .build(node_directory, chain_spec::gemini_2a().unwrap())
        .await
        .expect("error building the node");

    Ok(FarmingArgs {
        reward_address: config_args.farmer_config_args.reward_address,
        plot: config_args.farmer_config_args.plot,
        node,
    })
}
