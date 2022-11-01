use serde_derive::Deserialize;
use std::str::FromStr;
use std::{
    fs::{create_dir, File},
    path::PathBuf,
};
use thiserror::Error;

use subspace_sdk::{PlotDescription, PublicKey, Ss58ParsingError};

#[derive(Debug, Error)]
pub(crate) enum ConfigParseError {
    #[error("Cannot read the config file")]
    IO(#[from] std::io::Error),
    #[error("Cannot parse the config file")]
    Toml(#[from] toml::de::Error),
    #[error("SS58 parse error")]
    SS58Parse(#[from] Ss58ParsingError),
    #[error("Plot size parse error")]
    SizeParse(String),
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Config {
    farmer: FarmerConfig,
    node: NodeConfig,
    chains: ChainConfig,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FarmerConfig {
    address: String,
    sector_directory: PathBuf,
    sector_size: String,
    opencl: bool,
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
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
    pub(crate) _chain: String,
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
address = \"{reward_address}\"
sector_directory = \"{plot_location}\"
sector_size = \"{plot_size}\"
opencl = false

[node]
chain = \"{chain}\"
execution = \"wasm\"
blocks_pruning = 1024
state_pruning = 1024
validator = true
name = \"{node_name}\"
port = 30333
unsafe_ws_external = true # not sure we need this

[chains]
gemini_1 = \"rpc://1212312\"
gemini_2= \"rpc://\"
leo_3 = \"myown-network\"
dev = \"that local node experience\"
"
    )
}

pub(crate) fn parse_config() -> Result<ConfigArgs, ConfigParseError> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    let config_path = config_path.join("subspace-cli").join("settings.toml");

    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;

    let reward_address = PublicKey::from_str(&config.farmer.address)?;

    Ok(ConfigArgs {
        farmer_config_args: FarmingConfigArgs {
            reward_address,
            plot: PlotDescription {
                directory: config.farmer.sector_directory,
                space_pledged: config
                    .farmer
                    .sector_size
                    .parse::<bytesize::ByteSize>()
                    .map_err(ConfigParseError::SizeParse)?,
            },
        },
        node_config_args: NodeConfigArgs {
            name: config.node.name,
            _chain: config.node.chain,
        },
    })
}
