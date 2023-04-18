use std::fs::{create_dir_all, remove_file, File};
use std::path::PathBuf;

use color_eyre::eyre::{eyre, Report, Result, WrapErr};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use subspace_sdk::farmer::{CacheDescription, Farmer};
use subspace_sdk::node::domains::core::ConfigBuilder;
use subspace_sdk::node::{domains, DsnBuilder, NetworkBuilder, Node, Role};
use subspace_sdk::{chain_spec, ByteSize, PlotDescription, PublicKey};
use tracing::instrument;

use crate::utils::{cache_directory_getter, provider_storage_dir_getter};

/// defaults for the user config file
pub(crate) const DEFAULT_PLOT_SIZE: ByteSize = ByteSize::gb(1);
pub(crate) const MIN_PLOT_SIZE: ByteSize = ByteSize::mib(32);

/// structure of the config toml file
#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct Config {
    pub(crate) chain: ChainConfig,
    pub(crate) farmer: FarmerConfig,
    pub(crate) node: NodeConfig,
}

/// Advanced Node Settings Wrapper for CLI
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub(crate) struct AdvancedNodeSettings {
    #[serde(default, skip_serializing_if = "crate::utils::is_default")]
    pub(crate) executor: bool,
    #[serde(default, flatten)]
    pub(crate) extra: toml::Table,
}

/// Node Options Wrapper for CLI
#[derive(Deserialize, Serialize, Clone, Debug)]
pub(crate) struct NodeConfig {
    pub(crate) directory: PathBuf,
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "crate::utils::is_default")]
    pub(crate) advanced: AdvancedNodeSettings,
}

impl NodeConfig {
    pub async fn build(self, chain: ChainConfig, is_verbose: bool) -> Result<Node> {
        let Self { directory, name, advanced: AdvancedNodeSettings { executor, extra } } = self;

        let (mut node, chain_spec) = match chain {
            ChainConfig::Gemini3d => {
                let node = Node::gemini_3d().network(NetworkBuilder::gemini_3d().name(name)).dsn(
                    DsnBuilder::gemini_3d().provider_storage_path(provider_storage_dir_getter()),
                );
                let chain_spec = chain_spec::gemini_3d();
                (node, chain_spec)
            }
            ChainConfig::Dev => {
                let node = Node::dev();
                let chain_spec = chain_spec::dev_config();
                (node, chain_spec)
            }
            ChainConfig::DevNet => {
                let node = Node::devnet()
                    .network(NetworkBuilder::devnet().name(name))
                    .dsn(DsnBuilder::devnet().provider_storage_path(provider_storage_dir_getter()));
                let chain_spec = chain_spec::devnet_config();
                (node, chain_spec)
            }
        };

        if executor {
            node = node
                .system_domain(domains::ConfigBuilder::new().core(ConfigBuilder::new().build()));
        }

        if is_verbose {
            node = node.informant_enable_color(true);
        }

        node = node
            .role(Role::Authority)
            .impl_version(format!("{}-{}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH")))
            .impl_name("Subspace CLI".to_string());

        crate::utils::apply_extra_options(&node.configuration(), extra)
            .context("Failed to deserialize node config")?
            .build(directory, chain_spec)
            .await
            .map_err(color_eyre::Report::msg)
    }
}

/// Advanced Farmer Settings Wrapper for CLI
#[derive(Deserialize, Serialize, Clone, Derivative, Debug, PartialEq)]
#[derivative(Default)]
pub(crate) struct AdvancedFarmerSettings {
    #[serde(default, skip_serializing_if = "crate::utils::is_default")]
    #[derivative(Default(value = "subspace_sdk::ByteSize::gb(1)"))]
    pub(crate) cache_size: ByteSize,
    #[serde(default, flatten)]
    pub(crate) extra: toml::Table,
}

/// Farmer Options Wrapper for CLI
#[derive(Deserialize, Serialize, Clone, Debug)]
pub(crate) struct FarmerConfig {
    pub(crate) reward_address: PublicKey,
    pub(crate) plot_directory: PathBuf,
    pub(crate) plot_size: ByteSize,
    #[serde(default, skip_serializing_if = "crate::utils::is_default")]
    pub(crate) advanced: AdvancedFarmerSettings,
}

impl FarmerConfig {
    pub async fn build(self, node: &Node) -> Result<Farmer> {
        let plot_description = &[PlotDescription::new(self.plot_directory, self.plot_size)
            .wrap_err("Plot size is too low")?];
        let cache = CacheDescription::new(cache_directory_getter(), self.advanced.cache_size)?;

        // currently we do not have different configuration for the farmer w.r.t
        // different chains, but we may in the future
        let farmer = Farmer::builder();
        crate::utils::apply_extra_options(&farmer.configuration(), self.advanced.extra)
            .context("Failed to deserialize node config")?
            .build(self.reward_address, node, plot_description, cache)
            .await
            .context("Failed to build a farmer")
    }
}

/// Enum for Chain
#[derive(Deserialize, Serialize, Default, Clone, Debug, EnumIter)]
pub(crate) enum ChainConfig {
    #[default]
    Gemini3d,
    Dev,
    DevNet,
}

impl std::str::FromStr for ChainConfig {
    type Err = Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gemini3d" => Ok(ChainConfig::Gemini3d),
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

/// deletes the config file
#[instrument]
pub(crate) fn delete_config() -> Result<()> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    remove_file(config_path).context("couldn't delete config file")
}
