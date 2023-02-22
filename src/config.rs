use std::fs::{create_dir_all, File};
use std::path::PathBuf;
use std::str::FromStr;

use bytesize::ByteSize;
use color_eyre::eyre::{eyre, Result};
use color_eyre::Report;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use subspace_sdk::farmer::{CacheDescription, Config as SdkFarmerConfig, Farmer};
use subspace_sdk::node::domains::core::ConfigBuilder;
use subspace_sdk::node::{domains, Config as SdkNodeConfig, DsnBuilder, NetworkBuilder, Node};
use subspace_sdk::PublicKey;
use tracing::instrument;

use crate::utils::provider_storage_dir_getter;

/// defaults for the user config file
pub(crate) const DEFAULT_PLOT_SIZE: bytesize::ByteSize = bytesize::ByteSize::gb(1);
pub(crate) const MIN_PLOT_SIZE: bytesize::ByteSize = bytesize::ByteSize::mib(32);

/// structure of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct Config {
    pub(crate) chain: ChainConfig,
    pub(crate) farmer: FarmerConfig,
    pub(crate) node: NodeConfig,
}

/// structure for the `farmer` field of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct NodeConfig {
    pub(crate) directory: PathBuf,
    #[serde(flatten)]
    pub(crate) node: SdkNodeConfig,
}

impl NodeConfig {
    pub fn gemini_3c(directory: PathBuf, node_name: String, is_executor: bool) -> Self {
        let mut node = Node::gemini_3c()
            .network(NetworkBuilder::gemini_3c().name(node_name))
            .dsn(DsnBuilder::gemini_3c().provider_storage_path(provider_storage_dir_getter()));

        if is_executor {
            node = node
                .system_domain(domains::ConfigBuilder::new().core(ConfigBuilder::new().build()));
        }

        Self { directory, node: node.configuration() }
    }

    pub fn dev(directory: PathBuf, is_executor: bool) -> Self {
        let mut node = Node::dev();
        if is_executor {
            node = node
                .system_domain(domains::ConfigBuilder::new().core(ConfigBuilder::new().build()));
        }

        Self { directory, node: node.configuration() }
    }

    pub fn devnet(directory: PathBuf, node_name: String, is_executor: bool) -> Self {
        let mut node = Node::devnet()
            .network(NetworkBuilder::devnet().name(node_name))
            .dsn(DsnBuilder::devnet().provider_storage_path(provider_storage_dir_getter()));

        if is_executor {
            node = node
                .system_domain(domains::ConfigBuilder::new().core(ConfigBuilder::new().build()));
        }

        Self { directory, node: node.configuration() }
    }
}

/// structure for the `farmer` field of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct FarmerConfig {
    pub(crate) address: PublicKey,
    pub(crate) plot_directory: PathBuf,
    #[serde(with = "bytesize_serde")]
    pub(crate) plot_size: ByteSize,
    #[serde(flatten)]
    pub(crate) farmer: SdkFarmerConfig,
    pub(crate) cache: CacheDescription,
}

impl FarmerConfig {
    pub fn gemini_3c(
        address: PublicKey,
        plot_directory: PathBuf,
        plot_size: ByteSize,
        cache: CacheDescription,
    ) -> Self {
        Self {
            address,
            plot_directory,
            plot_size,
            cache,
            farmer: Farmer::builder().configuration(),
        }
    }

    pub fn dev(
        address: PublicKey,
        plot_directory: PathBuf,
        plot_size: ByteSize,
        cache: CacheDescription,
    ) -> Self {
        Self::gemini_3c(address, plot_directory, plot_size, cache)
    }

    pub fn devnet(
        address: PublicKey,
        plot_directory: PathBuf,
        plot_size: ByteSize,
        cache: CacheDescription,
    ) -> Self {
        Self::gemini_3c(address, plot_directory, plot_size, cache)
    }
}

#[derive(Deserialize, Serialize, Default, EnumIter, Debug)]
pub(crate) enum ChainConfig {
    #[default]
    Gemini3c,
    Dev,
    DevNet,
}

impl FromStr for ChainConfig {
    type Err = Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gemini3c" => Ok(ChainConfig::Gemini3c),
            "dev" => Ok(ChainConfig::Dev),
            "devnet" => Ok(ChainConfig::DevNet),
            _ => Err(eyre!("given chain: `{s}` is not recognized!")),
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

    if let Err(err) = create_dir_all(&config_path) {
        let config_path = config_path.to_str().expect("couldn't get subspace-cli config path!");
        return Err(eyre!("could not create the directory: `{config_path}`, because: {err}"));
    }

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
    if config.farmer.plot_size < MIN_PLOT_SIZE {
        return Err(eyre!("plot size should be bigger than {MIN_PLOT_SIZE}!"));
    }

    Ok(config)
}
