//! Config CLI command of pulsar is about setting the parameters:
//! - chain
//! - farm size
//! - reward address
//! - node directory
//! - farm directory

use std::fs;
use std::path::PathBuf;

use color_eyre::eyre::{self, eyre};
use subspace_sdk::ByteSize;

use crate::config::{parse_config, parse_config_path, ChainConfig, Config, MIN_FARM_SIZE};
use crate::utils::reward_address_parser;

// TODO: implement this
pub(crate) async fn config(
    chain: ChainConfig,
    show: bool,
    farm_size: ByteSize,
    reward_address: String,
    node_dir: PathBuf,
    farm_dir: PathBuf,
) -> eyre::Result<()> {
    // Define the path to your settings.toml file
    let config_path = parse_config_path()?;

    // if config file doesn't exist, then throw error
    if !config_path.exists() {
        return Err(eyre!(
            "Config file: \"settings.toml\" not found.\nPlease use `pulsar init` command first."
        ));
    }

    // Load the current configuration
    let mut config: Config = parse_config()?;

    if show {
        // Display the current configuration
        println!("Current Configuration: \n{:?}", config);
    } else {
        // Update the configuration based on the provided arguments
        if farm_size >= MIN_FARM_SIZE {
            config.farmer.farm_size = farm_size;
        } else {
            return Err(eyre!("Farm size must be ≥ 2 GB"));
        }

        let reward_address = reward_address_parser(&reward_address)?;
        config.farmer.reward_address = reward_address;

        if node_dir.exists() {
            config.node.directory = node_dir;
        }

        if farm_dir.exists() {
            config.farmer.farm_directory = farm_dir;
        }

        // Save the updated configuration back to the file
        fs::write(config_path, toml::to_string(&config)?)?;
    }

    Ok(())
}
