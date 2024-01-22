use futures::prelude::*;
use sdk_utils::ByteSize;

use crate::common::{Farmer, Node};

#[tokio::test(flavor = "multi_thread")]
async fn core_start() {
    crate::common::setup();

    let number_of_sectors = 10;
    let pieces_in_sector = 50u16;
    let sector_size = subspace_farmer_components::sector::sector_size(pieces_in_sector as _);
    let space_pledged = sector_size * number_of_sectors;

    let node = Node::dev().enable_core(true).build().await;
    let farmer = Farmer::dev()
        .pieces_in_sector(pieces_in_sector)
        .build(&node, ByteSize::b(space_pledged as u64))
        .await;

    node.system_domain()
        .unwrap()
        .payments()
        .unwrap()
        .subscribe_new_heads()
        .await
        .unwrap()
        .next()
        .await
        .unwrap();

    farmer.close().await;
    node.close().await;
}
