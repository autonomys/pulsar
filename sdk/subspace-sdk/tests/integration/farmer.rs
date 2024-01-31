use futures::prelude::*;
use subspace_sdk::utils::ByteSize;

use crate::common::{Farmer, Node};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "We need api from single disk plot to calculate precise target sector count"]
async fn track_progress() {
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

    let progress = farmer
        .iter_farms()
        .await
        .next()
        .unwrap()
        .subscribe_initial_plotting_progress()
        .await
        .collect::<Vec<_>>()
        .await;
    assert_eq!(progress.len(), number_of_sectors);

    farmer.close().await;
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn new_solution() {
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

    farmer
        .iter_farms()
        .await
        .next()
        .unwrap()
        .subscribe_new_solutions()
        .await
        .next()
        .await
        .expect("Farmer should send new solutions");

    farmer.close().await;
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn progress_restart() {
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

    let plot = farmer.iter_farms().await.next().unwrap();

    plot.subscribe_initial_plotting_progress().await.for_each(|_| async {}).await;

    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        plot.subscribe_initial_plotting_progress().await.for_each(|_| async {}),
    )
    .await
    .unwrap();

    farmer.close().await;
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Stack overflows for now"]
async fn farmer_restart() {
    crate::common::setup();

    let number_of_sectors = 10;
    let pieces_in_sector = 50u16;
    let sector_size = subspace_farmer_components::sector::sector_size(pieces_in_sector as _);
    let space_pledged = sector_size * number_of_sectors;

    let node = Node::dev().build(true).await;

    for _ in 0..10 {
        Farmer::dev()
            .pieces_in_sector(pieces_in_sector)
            .build(&node, ByteSize::b(space_pledged as u64))
            .await
            .close()
            .await;
    }

    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn farmer_close() {
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

    farmer.close().await;
    node.close().await;
}
