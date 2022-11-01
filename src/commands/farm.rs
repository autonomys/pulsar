use subspace_sdk::Farmer;
use subspace_sdk::{Node, PlotDescription, PublicKey};

use crate::config::parse_config;

pub(crate) struct FarmingArgs {
    reward_address: PublicKey,
    node: Node,
    plot: PlotDescription,
}

pub(crate) async fn farm() {
    let node: Node = Node::builder().build().await.unwrap();
    match get_args_for_farming(node) {
        Ok(args) => start_farming(args).await,
        Err(why) => println!("Error: {why}"),
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
        .unwrap();
}

fn get_args_for_farming(node: Node) -> Result<FarmingArgs, String> {
    match parse_config() {
        Ok(config_args) => Ok(FarmingArgs {
            reward_address: config_args.reward_address,
            plot: config_args.plot,
            node,
        }),
        Err(why) => Err(why),
    }
}
