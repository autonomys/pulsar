use std::fs::{create_dir, File};
use std::path::PathBuf;
use std::str::FromStr;

use bytesize::ByteSize;
use color_eyre::eyre::{eyre, Result};
use color_eyre::Report;
use serde::{Deserialize, Serialize};
use subspace_sdk::farmer::{CacheDescription, Config as SdkFarmerConfig, Farmer};
use subspace_sdk::node::{self, Config as SdkNodeConfig, Node};
use subspace_sdk::PublicKey;
use tracing::instrument;

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
    pub fn gemini_3b(directory: PathBuf, node_name: String) -> Self {
        Self {
            directory,
            node: Node::builder()
                .role(node::Role::Authority)
                .network(
                    node::NetworkBuilder::new()
                        .listen_addresses(vec![
                            "/ip6/::/tcp/30333".parse().unwrap(),
                            "/ip4/0.0.0.0/tcp/30333".parse().unwrap(),
                        ])
                        .name(node_name)
                        .enable_mdns(true),
                )
                .rpc(
                    node::RpcBuilder::new()
                        .http("127.0.0.1:9933".parse().unwrap())
                        .ws("127.0.0.1:9944".parse().unwrap())
                        .cors(vec![
                            "http://localhost:*".to_owned(),
                            "http://127.0.0.1:*".to_owned(),
                            "https://localhost:*".to_owned(),
                            "https://127.0.0.1:*".to_owned(),
                            "https://polkadot.js.org".to_owned(),
                        ]),
                )
                .dsn(node::DsnBuilder::new().listen_addresses(vec![
                    "/ip6/::/tcp/30433".parse().unwrap(),
                    "/ip4/0.0.0.0/tcp/30433".parse().unwrap(),
                ]))
                .execution_strategy(node::ExecutionStrategy::AlwaysWasm)
                .offchain_worker(node::OffchainWorkerBuilder::new().enabled(true))
                .configuration(),
        }
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
    pub fn gemini_3b(
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
}

#[derive(Deserialize, Serialize, Default)]
pub(crate) enum ChainConfig {
    #[default]
    Gemini3b,
}

impl std::fmt::Display for ChainConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            ChainConfig::Gemini3b => write!(f, "gemini-3b"),
        }
    }
}

impl FromStr for ChainConfig {
    type Err = Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let chain_list = vec!["gemini-3b"];
        match s {
            "gemini-3b" => Ok(ChainConfig::Gemini3b),
            _ => Err(eyre!(
                "given chain: `{s}` is not recognized! Please enter a valid chain from this list: \
                 {chain_list:?}."
            )),
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

    if let Err(err) = create_dir(&config_path) {
        // ignore the `AlreadyExists` error
        if err.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(eyre!("could not create the directory, because: {err}"));
        }
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_serializable() {
        toml::to_vec(&Config {
            farmer: FarmerConfig::gemini_3b(
                Default::default(),
                "plot".into(),
                ByteSize::gb(1),
                CacheDescription::new("cache", ByteSize::gb(1)).unwrap(),
            ),
            node: NodeConfig::gemini_3b("node".into(), "serializable-node".to_owned()),
            chain: ChainConfig::Gemini3b,
        })
        .unwrap();
    }
}
