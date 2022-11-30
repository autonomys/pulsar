use std::{
    fs::{create_dir, File},
    net::SocketAddr,
    path::PathBuf,
};

use bytesize::ByteSize;
use color_eyre::eyre::{eyre, Result};
use libp2p_core::Multiaddr;
use serde::{Deserialize, Serialize};
use tracing::instrument;

// TODO: delete this
use sc_network_common::config::MultiaddrWithPeerId;

use subspace_sdk::{
    farmer::CacheDescription,
    node::{ExecutionStrategy, Role, RpcMethods},
    PublicKey,
};

use crate::utils::chain_parser;

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
    pub(crate) cache: CacheDescription,
}

/// structure for the `node` field of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct NodeConfig {
    pub(crate) force_authoring: bool,
    pub(crate) role: Role,
    pub(crate) blocks_pruning: u32,
    pub(crate) state_pruning: u32,
    pub(crate) execution_strategies: ExecutionStrategy,
    #[serde(with = "bytesize_serde")]
    pub(crate) piece_cache_size: ByteSize,
    pub(crate) impl_name: String,
    pub(crate) impl_version: String,
    pub(crate) rpc: NodeRpcConfig,
    pub(crate) network: NodeNetworkConfig,
    pub(crate) dsn: NodeDsnConfig,
    pub(crate) chain: String,
}

/// `NodeRpcConfig` struct for `NodeConfig`
#[derive(Deserialize, Serialize)]
pub(crate) struct NodeRpcConfig {
    pub(crate) htpp: Option<SocketAddr>,
    pub(crate) ws: Option<SocketAddr>,
    pub(crate) ipc: Option<String>,
    pub(crate) ws_max_connections: Option<usize>,
    pub(crate) cors: Option<Vec<String>>,
    pub(crate) methods: RpcMethods,
    pub(crate) max_payload: Option<usize>,
    pub(crate) max_request_size: Option<usize>,
    pub(crate) max_response_size: Option<usize>,
    pub(crate) max_subs_per_conn: usize,
    pub(crate) ws_max_out_buffer_capacity: Option<usize>,
}

/// `NodeNetwork` struct for `NodeConfig`
#[derive(Deserialize, Serialize)]
pub(crate) struct NodeNetworkConfig {
    pub(crate) listen_addresses: Vec<Multiaddr>,
    pub(crate) boot_nodes: Vec<MultiaddrWithPeerId>,
    pub(crate) force_synced: bool,
    pub(crate) name: Option<String>,
    pub(crate) client_id: Option<String>,
}

/// `NodeDsnConfig` struct for `NodeConfig`
#[derive(Deserialize, Serialize)]
pub(crate) struct NodeDsnConfig {
    pub(crate) listen_addresses: Vec<Multiaddr>,
    pub(crate) boot_nodes: Vec<Multiaddr>,
    pub(crate) allow_non_global_addresses_in_dht: bool,
}

/// structure for the `chain` field of the config toml file
#[derive(Deserialize, Serialize)]
pub(crate) struct ChainConfig {
    pub(crate) dev: String,
    pub(crate) gemini_3a: String,
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
    if chain_parser(&config.node.chain).is_err() {
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
