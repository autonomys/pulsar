use std::str::FromStr;
use std::{
    fs::{create_dir, File},
    path::PathBuf,
};

use bytesize::ByteSize;
use color_eyre::eyre::{Report, Result};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use subspace_sdk::PublicKey;

/// structure of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct Config {
    pub(crate) farmer: FarmerConfig,
    pub(crate) node: NodeConfig,
    pub(crate) chains: ChainConfig,
}

/// structure for the `farmer` field of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct FarmerConfig {
    pub(crate) address: PublicKey,
    pub(crate) plot_directory: PathBuf,
    #[serde(with = "bytesize_serde")]
    pub(crate) plot_size: ByteSize,
    pub(crate) opencl: bool,
}

/// structure for the `node` field of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct NodeConfig {
    pub(crate) chain: String,
    pub(crate) execution: String,
    pub(crate) blocks_pruning: u32,
    pub(crate) state_pruning: u32,
    pub(crate) validator: bool,
    pub(crate) name: String,
    pub(crate) port: String,
    pub(crate) unsafe_ws_external: bool,
}

/// structure for the `chain` field of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct ChainConfig {
    pub(crate) dev: String,
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
            port: "".to_owned(),
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
pub(crate) fn parse_config() -> Result<Config> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    let config_path = config_path.join("subspace-cli").join("settings.toml");

    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;

    Ok(config)
}
