//! Config CLI command of pulsar is about setting the parameters:
//! - chain
//! - farm size
//! - reward address
//! - node directory
//! - farm directory

use std::fs;

use color_eyre::eyre::{self, bail};

use crate::config::{parse_config, parse_config_path, ChainConfig, Config, MIN_FARM_SIZE};
use crate::utils::{create_and_move_data, dir_parser, reward_address_parser, size_parser};

// TODO: implement this
pub(crate) async fn config(
    show: bool,
    chain: Option<ChainConfig>,
    farm_size: Option<String>,
    reward_address: Option<String>,
    node_dir: Option<String>,
    farm_dir: Option<String>,
) -> eyre::Result<()> {
    // Define the path to your settings.toml file
    let config_path = parse_config_path()?;

    // if config file doesn't exist, then throw error
    if !config_path.exists() {
        bail!("Config file: \"settings.toml\" not found.\nPlease use `pulsar init` command first.");
    }

    // Load the current configuration
    let mut config: Config = parse_config()?;

    if show {
        // Display the current configuration as JSON
        // Serialize `config` to a pretty-printed JSON string
        let serialized = serde_json::to_string_pretty(&config)?;
        println!(
            "Current Config set as: \n{}\n in file: {:?}",
            serialized,
            config_path.to_str().expect("Expected stringified config path")
        );
    } else {
        match chain {
            Some(c) => {
                // TODO: update (optional) the chain
            }
            None => match farm_size {
                Some(f) => {
                    // update (optional) the farm size
                    let farm_size = size_parser(&f)?;
                    // if let Ok(farm_size) = size_parser(&farm_size.unwrap()) {}
                    if farm_size >= MIN_FARM_SIZE {
                        config.farmer.farm_size = farm_size;
                    } else {
                        bail!("Farm size must be ≥ 2 GB");
                    }
                }
                None => match reward_address {
                    Some(r) => {
                        // update (optional) the reward address
                        let reward_address = reward_address_parser(&r)?;
                        config.farmer.reward_address = reward_address;
                    }
                    None => match node_dir {
                        Some(n) => {
                            // update (optional) the node directory
                            let node_dir = dir_parser(&n).expect("Invalid node directory");
                            create_and_move_data(config.node.directory.clone(), node_dir.clone())
                                .expect("Error in setting new node directory.");
                            config.node.directory = node_dir;
                        }
                        None => match farm_dir {
                            Some(fd) => {
                                // update (optional) the farm directory
                                let farm_dir = dir_parser(&fd).expect("Invalid farm directory");
                                create_and_move_data(
                                    config.farmer.farm_directory.clone(),
                                    farm_dir.clone(),
                                )
                                .expect("Error in setting new farm directory.");
                                if farm_dir.exists() {
                                    config.farmer.farm_directory = farm_dir;
                                }
                            }
                            None => {
                                println!(
                                    "At least one option has to be provided.\nTry `pulsar config \
                                     -h`"
                                );
                                return Ok(());
                            }
                        },
                    },
                },
            },
        }

        // Save the updated configuration back to the file
        fs::write(config_path, toml::to_string(&config)?)?;
    }

    Ok(())
}
