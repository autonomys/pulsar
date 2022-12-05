use std::{
    fs::{create_dir, File},
    path::PathBuf,
    str::FromStr,
};

use bytesize::ByteSize;
use color_eyre::{
    eyre::{eyre, Result},
    Report,
};
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use subspace_sdk::{
    farmer::{CacheDescription, Dsn as FarmerDsn},
    generate_builder,
    node::Config as NodeConfig,
    PublicKey,
};

/// structure of the config toml file
#[derive(Deserialize, Serialize, Builder)]
#[builder(pattern = "owned", build_fn(name = "_build"))]
pub(crate) struct Config {
    pub(crate) chain: ChainConfig,
    #[builder(setter(into))]
    pub(crate) farmer: FarmerConfig,
    #[builder(setter(into))]
    pub(crate) node: NodeConfig,
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
    #[builder(default)]
    #[serde(default)]
    pub(crate) opencl: bool,
    pub(crate) cache: CacheDescription,
    #[builder(setter(into))]
    pub(crate) dsn: FarmerDsn,
}

generate_builder!(FarmerConfig);

#[derive(Deserialize, Serialize, Default)]
pub(crate) enum ChainConfig {
    #[default]
    Gemini3a,
    Dev,
}

impl std::fmt::Display for ChainConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            ChainConfig::Dev => write!(f, "dev"),
            ChainConfig::Gemini3a => write!(f, "gemini-3a"),
        }
    }
}

impl FromStr for ChainConfig {
    type Err = Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let chain_list = vec!["dev", "gemini-3a"];
        match s {
            "dev" => Ok(ChainConfig::Dev),
            "gemini-3a" => Ok(ChainConfig::Gemini3a),
            _ => Err(eyre!("given chain: `{s}` is not recognized! Please enter a valid chain from this list: {chain_list:?}.")),
        }
    }
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
    let Some(ref name) = config.node.network.name else {
        return Err(eyre!("Node name was `None`"));
    };
    if name.trim().is_empty() {
        return Err(eyre!("Node nome is empty"));
    }

    Ok(config)
}
