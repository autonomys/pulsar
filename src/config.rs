use color_eyre::eyre::{Report, Result};
use serde::Serialize;
use serde_derive::Deserialize;
use std::str::FromStr;
use std::{
    fs::{create_dir, File},
    path::PathBuf,
};
use tracing::instrument;

use subspace_sdk::{PlotDescription, PublicKey};

#[derive(Deserialize, Serialize)]
#[allow(dead_code)]
struct Config {
    farmer: FarmerConfig,
    node: NodeConfig,
    chains: ChainConfig,
}

#[derive(Deserialize, Serialize)]
#[allow(dead_code)]
struct FarmerConfig {
    address: String,
    sector_directory: String,
    sector_size: String,
    opencl: bool,
}

#[derive(Deserialize, Serialize)]
#[allow(dead_code)]
struct NodeConfig {
    chain: String,
    execution: String,
    blocks_pruning: usize,
    state_pruning: usize,
    validator: bool,
    name: String,
    port: usize,
    unsafe_ws_external: bool,
}

#[derive(Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainConfig {
    gemini_1: String,
    gemini_2: String,
    leo_3: String,
    dev: String,
}

pub(crate) struct ConfigArgs {
    pub(crate) farmer_config_args: FarmingConfigArgs,
    pub(crate) node_config_args: NodeConfigArgs,
}

pub(crate) struct FarmingConfigArgs {
    pub(crate) reward_address: PublicKey,
    pub(crate) plot: PlotDescription,
}

pub(crate) struct NodeConfigArgs {
    pub(crate) name: String,
    pub(crate) chain: String,
}

/// Creates a config file at the location
/// - **Linux:** `$HOME/.config/subspace-cli/settings.toml`.
/// - **macOS:** `$HOME/Library/Application Support/subspace-cli/settings.toml`.
/// - **Windows:** `{FOLDERID_RoamingAppData}/subspace-cli/settings.toml`.
pub(crate) fn create_config() -> Result<(File, PathBuf)> {
    let config_path = dirs::config_dir()
        .expect("couldn't get the default config directory!")
        .join("subspace-cli");

    let _ = create_dir(config_path.clone()); // if folder already exists, ignore the error

    let file = File::create(config_path.join("settings.toml"))?;

    Ok((file, config_path))
}

pub(crate) fn construct_config(
    reward_address: &str,
    plot_location: &str,
    plot_size: &str,
    chain: &str,
    node_name: &str,
) -> Result<String, toml::ser::Error> {
    let config = Config {
        farmer: FarmerConfig {
            address: reward_address.to_owned(),
            sector_directory: plot_location.to_owned(),
            sector_size: plot_size.to_owned(),
            opencl: false,
        },
        node: NodeConfig {
            chain: chain.to_owned(),
            execution: "wasm".to_owned(),
            blocks_pruning: 1024,
            state_pruning: 1024,
            validator: true,
            name: node_name.to_owned(),
            port: 30333,
            unsafe_ws_external: true, // not sure we need this
        },
        chains: ChainConfig {
            gemini_1: "rpc://1212312".to_owned(),
            gemini_2: "rpc://".to_owned(),
            leo_3: "myown_network".to_owned(),
            dev: "that local node experience".to_owned(),
        },
    };

    toml::to_string(&config)
}

#[instrument]
pub(crate) fn parse_config() -> Result<ConfigArgs> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    let config_path = config_path.join("subspace-cli").join("settings.toml");

    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;
    let reward_address = PublicKey::from_str(&config.farmer.address)?;
    let directory = PathBuf::from_str(&config.farmer.sector_directory)?;
    let space_pledged = config
        .farmer
        .sector_size
        .parse::<bytesize::ByteSize>()
        .map_err(Report::msg)?;

    Ok(ConfigArgs {
        farmer_config_args: FarmingConfigArgs {
            reward_address,
            plot: PlotDescription {
                directory,
                space_pledged,
            },
        },
        node_config_args: NodeConfigArgs {
            name: config.node.name,
            chain: config.node.chain,
        },
    })
}
