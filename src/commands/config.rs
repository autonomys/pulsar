//! Config CLI command of pulsar is about setting the parameters:
//! - chain
//! - farm size
//! - reward address
//! - node directory
//! - farm directory

use std::path::PathBuf;

use color_eyre::eyre;
use sp_core::sr25519::Public;
use subspace_sdk::ByteSize;

use crate::config::{ChainConfig, MIN_FARM_SIZE};

// TODO: implement this
pub(crate) async fn config(
    chain: ChainConfig,
    show: bool,
    farm_size: ByteSize,
    reward_address: Option<Public>,
    node_dir: PathBuf,
    farm_dir: PathBuf,
) -> eyre::Result<()> {
    // ensure `settings.toml`file from the dir (as per OS) & then fetch
    // let mut config = toml

    // Handle the `show` subcommand
    // match show {
    //     true => {

    //         // Logic to display the current configuration
    //     }
    //     // false => {
    //         // Handle the `farm_size` subcommand
    //         if farm_size < MIN_FARM_SIZE {
    //             eyre
    //         } else {
    //             // Additional logic for `farm_size`
    //             // config.farm_size = farm_size
    //         }

    //         // Handle the `reward_address` subcommand
    //         if let Some(address) = reward_address {
    //             // Logic for handling the reward address
    //         }

    //         // Handle `node_dir` and `farm_dir` similarly...
    //     }
    // };

    Ok(())
}
