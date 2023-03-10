use std::fs::{create_dir_all, File};
use std::path::PathBuf;
use std::str::FromStr;

use bytesize::ByteSize;
use color_eyre::eyre::{eyre, Result, WrapErr};
use color_eyre::Report;
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
// use strum_macros::EnumIter; // uncomment this when gemini3d releases
use subspace_sdk::farmer::{CacheDescription, Farmer};
use subspace_sdk::node::domains::core::ConfigBuilder;
use subspace_sdk::node::{domains, DsnBuilder, NetworkBuilder, Node, Role};
use subspace_sdk::{chain_spec, PlotDescription, PublicKey};
use tracing::instrument;

use crate::utils::{cache_directory_getter, provider_storage_dir_getter};

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

/// Advanced Node Settings Wrapper for CLI
#[derive(Deserialize, Serialize, Builder, Clone)]
#[builder(setter(strip_option))]
pub(crate) struct AdvancedNodeSettings {
    pub(crate) executor: Option<bool>,
}

/// Node Options Wrapper for CLI
#[derive(Deserialize, Serialize, Builder, Clone)]
#[builder(build_fn(private, name = "_constructor"), name = "NodeBuilder")]
pub(crate) struct NodeConfig {
    pub(crate) directory: PathBuf,
    pub(crate) name: String,
    pub(crate) advanced: AdvancedNodeSettings,
}

impl NodeBuilder {
    pub fn configuration(&self) -> NodeConfig {
        self._constructor().expect("build is infallible")
    }

    pub async fn build(self, chain: ChainConfig) -> Result<Node> {
        self.configuration().build(chain).await
    }
}

impl NodeConfig {
    pub async fn build(self, chain: ChainConfig) -> Result<Node> {
        let mut node;
        match chain {
            ChainConfig::Gemini3c => {
                node = Node::gemini_3c().network(NetworkBuilder::gemini_3c().name(self.name)).dsn(
                    DsnBuilder::gemini_3c().provider_storage_path(provider_storage_dir_getter()),
                );

                if self.advanced.executor.unwrap_or(false) {
                    node = node.system_domain(
                        domains::ConfigBuilder::new().core(ConfigBuilder::new().build()),
                    );
                }
                node = node.role(Role::Authority);
                let chain_spec = chain_spec::gemini_3c()
                    .expect("cannot extract the gemini3c chain spec from SDK");

                node.build(self.directory, chain_spec).await.map_err(color_eyre::Report::msg)
            }
            ChainConfig::Dev => {
                node = Node::dev();

                if self.advanced.executor.unwrap_or_default() {
                    node = node.system_domain(
                        domains::ConfigBuilder::new().core(ConfigBuilder::new().build()),
                    );
                }
                node = node.role(Role::Authority);
                let chain_spec = chain_spec::gemini_3c()
                    .expect("cannot extract the gemini3c chain spec from SDK");

                node.build(self.directory, chain_spec).await.map_err(color_eyre::Report::msg)
            }
            ChainConfig::DevNet => {
                let mut node = Node::devnet()
                    .network(NetworkBuilder::devnet().name(self.name))
                    .dsn(DsnBuilder::devnet().provider_storage_path(provider_storage_dir_getter()));

                if self.advanced.executor.unwrap_or_default() {
                    node = node.system_domain(
                        domains::ConfigBuilder::new().core(ConfigBuilder::new().build()),
                    );
                }
                node = node.role(Role::Authority);
                let chain_spec = chain_spec::gemini_3c()
                    .expect("cannot extract the gemini3c chain spec from SDK");

                node.build(self.directory, chain_spec).await.map_err(color_eyre::Report::msg)
            }
        }
    }
}

/// Advanced Farmer Settings Wrapper for CLI
#[derive(Deserialize, Serialize, Builder, Clone)]
#[builder(setter(strip_option))]
pub(crate) struct AdvancedFarmerSettings {
    cache_size: Option<u64>,
}

/// Farmer Options Wrapper for CLI
#[derive(Deserialize, Serialize, Builder, Clone)]
#[builder(build_fn(name = "constructor"), name = "FarmerBuilder")]
pub(crate) struct FarmerConfig {
    pub(crate) address: PublicKey,
    pub(crate) plot_directory: PathBuf,
    #[serde(with = "bytesize_serde")]
    pub(crate) plot_size: ByteSize,
    pub(crate) advanced: AdvancedFarmerSettings,
}

impl FarmerBuilder {
    pub fn configuration(&self) -> FarmerConfig {
        self.constructor().expect("build is infallible")
    }

    pub async fn build(self, chain: ChainConfig, node: Node) {
        self.configuration().build(chain, node).await;
    }
}

impl FarmerConfig {
    pub async fn build(self, chain: ChainConfig, node: Node) -> Result<Farmer> {
        let plot_description = &[PlotDescription::new(self.plot_directory, self.plot_size)
            .wrap_err("Plot size is too low")?];
        let cache = CacheDescription::new(
            cache_directory_getter(),
            bytesize::ByteSize::gb(self.advanced.cache_size.unwrap_or(1)),
        )?;
        // currently we do not have different configuration for the farmer w.r.t
        // different chains, but we may in the future
        match chain {
            ChainConfig::Gemini3c => Farmer::builder()
                .build(self.address, node, plot_description, cache)
                .await
                .map_err(Report::msg),
            ChainConfig::Dev => Farmer::builder()
                .build(self.address, node, plot_description, cache)
                .await
                .map_err(Report::msg),
            ChainConfig::DevNet => Farmer::builder()
                .build(self.address, node, plot_description, cache)
                .await
                .map_err(Report::msg),
        }
    }
}

/// Enum for Chain
#[derive(Deserialize, Serialize, Default)] // TODO: add `EnumIter` when gemini3d releases
pub(crate) enum ChainConfig {
    #[default]
    Gemini3c,
    Dev,
    DevNet,
}

// TODO: delete this when gemini3d releases
impl std::fmt::Display for ChainConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            ChainConfig::Gemini3c => write!(f, "gemini-3c"),
            ChainConfig::Dev => write!(f, "dev-chain"),
            ChainConfig::DevNet => write!(f, "devnet"),
        }
    }
}

impl FromStr for ChainConfig {
    type Err = Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gemini-3c" => Ok(ChainConfig::Gemini3c),
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
