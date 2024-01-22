use std::sync::Arc;

use futures::prelude::*;
use subspace_sdk::utils::ByteSize;
use tempfile::TempDir;
use tracing_futures::Instrument;

use crate::common::{Farmer, Node};

async fn sync_block_inner() {
    crate::common::setup();

    let number_of_sectors = 10;
    let pieces_in_sector = 50u16;
    let sector_size = subspace_farmer_components::sector::sector_size(pieces_in_sector as _);
    let space_pledged = sector_size * number_of_sectors;

    let node = Node::dev().build(true).await;
    let farmer = Farmer::dev()
        .pieces_in_sector(pieces_in_sector)
        .build(&node, ByteSize::b(space_pledged as u64))
        .await;

    let farm_blocks = 5;

    node.subscribe_new_heads()
        .await
        .unwrap()
        .skip_while(|notification| futures::future::ready(notification.number < farm_blocks))
        .next()
        .await
        .unwrap();

    farmer.close().await;

    let other_node = Node::dev()
        .chain(node.chain.clone())
        .boot_nodes(node.listen_addresses().await.unwrap())
        .not_force_synced(true)
        .not_authority(true)
        .build(false)
        .await;

    other_node.subscribe_syncing_progress().await.unwrap().for_each(|_| async {}).await;
    assert_eq!(other_node.get_info().await.unwrap().best_block.1, farm_blocks);

    node.close().await;
    other_node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
//#[cfg_attr(any(tarpaulin, not(target_os = "linux")), ignore = "Slow tests are
//#[cfg_attr(any(tarpaulin, run only on linux")]
async fn sync_block() {
    tokio::time::timeout(std::time::Duration::from_secs(60 * 60), sync_block_inner()).await.unwrap()
}

async fn sync_farm_inner() {
    crate::common::setup();

    let number_of_sectors = 10;
    let pieces_in_sector = 50u16;
    let sector_size = subspace_farmer_components::sector::sector_size(pieces_in_sector as _);
    let space_pledged = sector_size * number_of_sectors;

    let node_span = tracing::trace_span!("node 1");
    let node = Node::dev().build(true).instrument(node_span.clone()).await;

    let farmer = Farmer::dev()
        .pieces_in_sector(pieces_in_sector)
        .build(&node, ByteSize::b(space_pledged as u64))
        .instrument(node_span.clone())
        .await;

    let farm_blocks = 4;

    node.subscribe_new_heads()
        .await
        .unwrap()
        .skip_while(|notification| futures::future::ready(notification.number < farm_blocks))
        .next()
        .await
        .unwrap();

    let other_node_span = tracing::trace_span!("node 2");
    let other_node = Node::dev()
        .dsn_boot_nodes(node.dsn_listen_addresses().await.unwrap())
        .boot_nodes(node.listen_addresses().await.unwrap())
        .not_force_synced(true)
        .chain(node.chain.clone())
        .build(false)
        .instrument(other_node_span.clone())
        .await;

    while other_node.get_info().await.unwrap().best_block.1
        < node.get_info().await.unwrap().best_block.1
    {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let other_farmer = Farmer::dev()
        .pieces_in_sector(pieces_in_sector)
        .build(&other_node, ByteSize::b(space_pledged as u64))
        .instrument(other_node_span.clone())
        .await;

    let farm = other_farmer.iter_farms().await.next().unwrap();
    farm.subscribe_initial_plotting_progress().await.for_each(|_| async {}).await;
    farmer.close().await;

    farm.subscribe_new_solutions().await.next().await.expect("Solution stream never ends");

    node.close().await;
    other_node.close().await;
    other_farmer.close().await;
}

#[tokio::test(flavor = "multi_thread")]
//#[cfg_attr(any(tarpaulin, not(target_os = "linux")), ignore = "Slow tests are
//#[cfg_attr(any(tarpaulin, run only on linux")]
async fn sync_farm() {
    tokio::time::timeout(std::time::Duration::from_secs(60 * 60), sync_farm_inner()).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Substrate rpc server doesn't let node to properly exit"]
async fn node_restart() {
    crate::common::setup();
    let dir = Arc::new(TempDir::new().unwrap());

    for i in 0..4 {
        tracing::error!(i, "Running new node");
        Node::dev().path(dir.clone()).build(true).await.close().await;
    }
}

#[tokio::test(flavor = "multi_thread")]
//#[cfg_attr(any(tarpaulin, not(target_os = "linux")), ignore = "Slow tests are
//#[cfg_attr(any(tarpaulin, run only on linux")]
async fn node_events() {
    crate::common::setup();

    tokio::time::timeout(std::time::Duration::from_secs(30 * 60), async {
        let number_of_sectors = 10;
        let pieces_in_sector = 50u16;
        let sector_size = subspace_farmer_components::sector::sector_size(pieces_in_sector as _);
        let space_pledged = sector_size * number_of_sectors;

        let node = Node::dev().build(true).await;
        let farmer = Farmer::dev()
            .pieces_in_sector(pieces_in_sector)
            .build(&node, ByteSize::b(space_pledged as u64))
            .await;

        let events = node
            .subscribe_new_heads()
            .await
            .unwrap()
            // Skip genesis
            .skip(1)
            .then(|_| node.get_events(None).boxed())
            .take(1)
            .next()
            .await
            .unwrap()
            .unwrap();

        assert!(!events.is_empty());

        farmer.close().await;
        node.close().await;
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
//#[cfg_attr(any(tarpaulin, not(target_os = "linux")), ignore = "Slow tests are
//#[cfg_attr(any(tarpaulin, run only on linux")]
async fn fetch_block_author() {
    crate::common::setup();

    tokio::time::timeout(std::time::Duration::from_secs(30 * 60), async {
        let number_of_sectors = 10;
        let pieces_in_sector = 50u16;
        let sector_size = subspace_farmer_components::sector::sector_size(pieces_in_sector as _);
        let space_pledged = sector_size * number_of_sectors;

        let node = Node::dev().build(false).await;
        let reward_address = Default::default();
        let farmer = Farmer::dev()
            .reward_address(reward_address)
            .pieces_in_sector(pieces_in_sector)
            .build(&node, ByteSize::b(space_pledged as u64))
            .await;

        let block = node.subscribe_new_heads().await.unwrap().skip(1).take(1).next().await.unwrap();
        assert_eq!(block.pre_digest.unwrap().solution().reward_address, reward_address);

        farmer.close().await;
        node.close().await;
    })
    .await
    .unwrap();
}
