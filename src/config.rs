use std::str::FromStr;
use std::{
    fs::{create_dir, File},
    path::PathBuf,
};

use bytesize::ByteSize;
use color_eyre::eyre::{Report, Result};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use subspace_sdk::{PlotDescription, PublicKey};

/// structure of the config toml file
#[derive(Deserialize, Serialize)]
struct Config {
    farmer: FarmerConfig,
    node: NodeConfig,
    chains: ChainConfig,
}

/// structure for the `farmer` field of the config toml file
#[derive(Deserialize, Serialize)]
struct FarmerConfig {
    address: PublicKey,
    plot_directory: PathBuf,
    #[serde(with = "bytesize_serde")]
    plot_size: ByteSize,
    opencl: bool,
}

/// structure for the `node` field of the config toml file
#[derive(Deserialize, Serialize)]
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

/// structure for the `chain` field of the config toml file
#[derive(Deserialize, Serialize)]
struct ChainConfig {
    dev: String,
}

/// struct to be returned from the [`parse_config`]
///
/// when we need all the fields of the config toml file,
/// this may become unnecessary
pub(crate) struct ConfigArgs {
    pub(crate) farmer_config_args: FarmingConfigArgs,
    pub(crate) node_config_args: NodeConfigArgs,
}

/// inner struct of the [`ConfigArgs`]
pub(crate) struct FarmingConfigArgs {
    pub(crate) reward_address: PublicKey,
    pub(crate) plot: PlotDescription,
}

/// inner struct of the [`ConfigArgs`]
pub(crate) struct NodeConfigArgs {
    pub(crate) name: String,
    pub(crate) chain: String,
    pub(crate) validator: bool,
}

/// Creates a config file at the location
/// - **Linux:** `$HOME/.config/subspace-cli/settings.toml`.
/// - **macOS:** `$HOME/Library/Application Support/subspace-cli/settings.toml`.
/// - **Windows:** `{FOLDERID_RoamingAppData}/subspace-cli/settings.toml`.
pub(crate) fn create_config() -> Result<(File, PathBuf)> {
    let config_path = dirs::config_dir()
        .expect("couldn't get the default config directory!")
        .join("subspace-cli");

    let _ = create_dir(&config_path); // if folder already exists, ignore the error

    let file = File::create(config_path.join("settings.toml"))?;

    Ok((file, config_path))
}

/// constructs the config toml file
///
/// some of the values are initialized with their default values
/// these may be configurable in the future
pub(crate) fn construct_config(
    reward_address: &str,
    plot_location: &str,
    plot_size: &str,
    chain: &str,
    node_name: &str,
) -> Result<String> {
    let config = Config {
        farmer: FarmerConfig {
            address: PublicKey::from_str(reward_address)?,
            plot_directory: PathBuf::from_str(plot_location)?,
            plot_size: plot_size
                .parse::<bytesize::ByteSize>()
                .map_err(Report::msg)?,
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
            dev: "that local node experience".to_owned(),
        },
    };

    toml::to_string(&config).map_err(Report::msg)
}

/// parses the config, and returns [`ConfigArgs`]
#[instrument]
pub(crate) fn parse_config() -> Result<ConfigArgs> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    let config_path = config_path.join("subspace-cli").join("settings.toml");

    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;

    Ok(ConfigArgs {
        farmer_config_args: FarmingConfigArgs {
            reward_address: config.farmer.address,
            plot: PlotDescription {
                directory: config.farmer.plot_directory,
                space_pledged: config.farmer.plot_size,
            },
        },
        node_config_args: NodeConfigArgs {
            name: config.node.name,
            chain: config.node.chain,
            validator: config.node.validator,
        },
    })
}
