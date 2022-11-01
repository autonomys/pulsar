use serde_derive::Deserialize;
use std::str::FromStr;
use std::{
    fs::{create_dir, File},
    path::PathBuf,
};
use thiserror::Error;

use subspace_sdk::{PlotDescription, PublicKey};

#[derive(Debug, Error)]
pub(crate) enum ConfigParseError {
    #[error("IO error")]
    IOError(#[from] std::io::Error),
    #[error("Toml error")]
    TomlError(#[from] toml::de::Error),
    //#[error("SS58 parse error")]
    //SS58ParseError(#[from] SS58ParseError)
}

#[derive(Deserialize)]
struct Config {
    farmer: FarmerConfig,
    _node: NodeConfig,
    _chains: NodeConfig,
}

#[derive(Deserialize)]
struct FarmerConfig {
    address: String,
    sector_directory: PathBuf,
    sector_size: String,
    _opencl: bool,
}

#[derive(Deserialize)]
struct NodeConfig {
    _chain: String,
    _execution: String,
    _blocks_pruning: usize,
    _state_pruning: usize,
    _validator: bool,
    _name: String,
    _port: usize,
    _unsafe_ws_external: bool,
}

#[derive(Deserialize)]
struct ChainConfig {
    _gemini_1: String,
    _gemini_2: String,
    _leo_3: String,
    _dev: String,
}

pub(crate) struct FarmingConfigArgs {
    pub(crate) reward_address: PublicKey,
    pub(crate) plot: PlotDescription,
}

/// Creates a config file at the location
/// - **Linux:** `$HOME/.config/subspace-cli/settings.toml`.
/// - **macOS:** `$HOME/Library/Application Support/subspace-cli/settings.toml`.
/// - **Windows:** `{FOLDERID_RoamingAppData}/subspace-cli/settings.toml`.
pub(crate) fn create_config() -> (File, PathBuf) {
    let config_path = match dirs::config_dir() {
        Some(path) => path,
        None => panic!("couldn't get the default config directory!"),
    };
    let config_path = config_path.join("subspace-cli");
    let _ = create_dir(config_path.clone()); // if folder already exists, ignore the error

    match File::create(config_path.join("settings.toml")) {
        Err(why) => panic!("couldn't create the config file because: {}", why),
        Ok(file) => (file, config_path),
    }
}

pub(crate) fn construct_config(
    reward_address: &str,
    plot_location: &str,
    plot_size: &str,
    chain: &str,
    node_name: &str,
) -> String {
    format!(
        "[farmer]
address = \"{}\"
sector_directory = \"{}\"
sector_size = \"{}\"
opencl = false

[node]
chain = \"{}\"
execution = \"wasm\"
blocks-pruning = 1024
state-pruning = 1024
validator = true
name = \"{}\"
port = 30333
unsafe-ws-external = true # not sure we need this

[chains]
gemini-1 = \"rpc://1212312\"
gemini-2= \"rpc://\"
leo-3 = \"myown-network\"
dev = \"that local node experience\"
",
        reward_address, plot_location, plot_size, chain, node_name
    )
}

pub(crate) fn parse_config() -> Result<FarmingConfigArgs, ConfigParseError> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    let config_path = config_path.join("subspace-cli");

    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;

    let reward_address = PublicKey::from_str(&config.farmer.address)?;

    Ok(FarmingConfigArgs {
        reward_address: reward_address,
        plot: PlotDescription {
            directory: config.farmer.sector_directory,
            space_pledged: config
                .farmer
                .sector_size
                .parse::<bytesize::ByteSize>()
                .expect("Plot size in config is malformed"),
        },
    })
}
