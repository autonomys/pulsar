use std::num::NonZeroU8;

use futures::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let plots = [subspace_sdk::FarmDescription::new("plot", subspace_sdk::ByteSize::mb(100))];
    let node = subspace_sdk::Node::builder()
        .force_authoring(true)
        .role(subspace_sdk::node::Role::Authority)
        // Starting a new chain
        .build("node", subspace_sdk::chain_spec::dev_config())
        .await
        .unwrap();

    let farmer = subspace_sdk::Farmer::builder()
        .build(
            subspace_sdk::PublicKey::from([0; 32]),
            &node,
            &plots,
            NonZeroU8::new(1).expect("Static value should not fail; qed"),
        )
        .await
        .expect("Failed to init a farmer");

    for plot in farmer.iter_farms().await {
        let mut plotting_progress = plot.subscribe_initial_plotting_progress().await;
        while plotting_progress.next().await.is_some() {}
    }
    tracing::info!("Initial plotting completed");

    node.subscribe_new_heads()
        .await
        .unwrap()
        // Wait 10 blocks and exit
        .take(10)
        .for_each(|header| async move { tracing::info!(?header, "New block!") })
        .await;
}
