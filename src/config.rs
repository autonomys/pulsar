use std::{
    fs::{create_dir, File},
    path::PathBuf,
};

use bytesize::ByteSize;
use color_eyre::eyre::{eyre, Result};
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use subspace_sdk::{
    farmer::{CacheDescription, Dsn as FarmerDsn},
    generate_builder,
    node::Config as NodeConfig,
    PublicKey,
};

use crate::utils::chain_parser;

/// structure of the config toml file
#[derive(Deserialize, Serialize, Builder)]
#[builder(pattern = "owned", build_fn(name = "_build"))]
pub(crate) struct Config {
    pub(crate) farmer: FarmerConfig,
    pub(crate) node: NodeConfig,
    pub(crate) chain: String,
}

generate_builder!(Config);

/// structure for the `farmer` field of the config toml file
#[derive(Deserialize, Serialize, Builder)]
#[builder(pattern = "owned", build_fn(name = "_build"))]
pub(crate) struct FarmerConfig {
    pub(crate) address: PublicKey,
    pub(crate) plot_directory: PathBuf,
    #[serde(with = "bytesize_serde")]
    pub(crate) plot_size: ByteSize,
    pub(crate) opencl: bool,
    pub(crate) cache: CacheDescription,
    pub(crate) dsn: FarmerDsn,
}

generate_builder!(FarmerConfig);

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

/// parses the config, and returns [`Config`]
#[instrument]
pub(crate) fn parse_config() -> Result<Config> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    let config_path = config_path.join("subspace-cli").join("settings.toml");

    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;
    Ok(config)
}

/// validates the config for farming
#[instrument]
pub(crate) fn validate_config() -> Result<Config> {
    let config = parse_config()?;

    // validity checks
    if config.farmer.plot_size < ByteSize::gb(1) {
        return Err(eyre!("plot size should be bigger than 1GB!"));
    }
    if chain_parser(&config.chain).is_err() {
        return Err(eyre!("chain is not recognized!"));
    }
    let Some(name) = config.node.network.name else {
        return Err(eyre!("Node name was `None`"));
    };
    if name.trim().is_empty() {
        return Err(eyre!("Node nome is empty"));
    }

    Ok(config)
}
