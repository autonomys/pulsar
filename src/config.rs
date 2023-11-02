use std::fs::{create_dir_all, remove_file, File};
use std::num::NonZeroU8;
use std::path::PathBuf;

use color_eyre::eyre::{eyre, Report, Result, WrapErr};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use sp_core::crypto::{AccountId32, Ss58Codec};
use strum_macros::EnumIter;
use subspace_sdk::farmer::Farmer;
use subspace_sdk::node::{
    DomainConfigBuilder, DsnBuilder, NetworkBuilder, Node, Role,
};
use subspace_sdk::{chain_spec, ByteSize, FarmDescription, PublicKey};
use tracing::instrument;

use crate::utils::{provider_storage_dir_getter, IntoEyre};

/// defaults for the user config file
pub(crate) const DEFAULT_FARM_SIZE: ByteSize = ByteSize::gb(2);
pub(crate) const MIN_FARM_SIZE: ByteSize = ByteSize::gb(2);

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
    pub(crate) enable_domains: bool,
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
        let Self { directory, name, advanced: AdvancedNodeSettings { enable_domains, extra } } =
            self;

        let (mut node, chain_spec) = match chain {
            ChainConfig::Gemini3g => {
                let mut node =
                    Node::gemini_3g().network(NetworkBuilder::gemini_3g().name(name)).dsn(
                        DsnBuilder::gemini_3g()
                            .provider_storage_path(provider_storage_dir_getter()),
                    );
                if enable_domains {
                    node = node.domain(Some(
                        DomainConfigBuilder::gemini_3g()
                            .relayer_id(
                                AccountId32::from_ss58check(
                                    "5CXTmJEusve5ixyJufqHThmy4qUrrm6FyLCR7QfE4bbyMTNC",
                                )
                                .expect("Static address should not fail"),
                            )
                            .configuration(),
                    ));
                }
                let chain_spec = chain_spec::gemini_3g();
                (node, chain_spec)
            }
            ChainConfig::Dev => {
                let mut node = Node::dev();
                if enable_domains {
                    node = node.domain(Some(
                        DomainConfigBuilder::dev()
                            .role(Role::Authority)
                            .relayer_id(
                                AccountId32::from_ss58check(
                                    "5CXTmJEusve5ixyJufqHThmy4qUrrm6FyLCR7QfE4bbyMTNC",
                                )
                                .expect("Static address should not fail"),
                            )
                            .configuration(),
                    ));
                }
                let chain_spec = chain_spec::dev_config();
                (node, chain_spec)
            }
            ChainConfig::DevNet => {
                let mut node = Node::devnet()
                    .network(NetworkBuilder::devnet().name(name))
                    .dsn(DsnBuilder::devnet().provider_storage_path(provider_storage_dir_getter()));
                if enable_domains {
                    node = node.domain(Some(
                        DomainConfigBuilder::devnet()
                            .relayer_id(
                                AccountId32::from_ss58check(
                                    "5CXTmJEusve5ixyJufqHThmy4qUrrm6FyLCR7QfE4bbyMTNC",
                                )
                                .expect("Static address should not fail"),
                            )
                            .configuration(),
                    ));
                }
                let chain_spec = chain_spec::devnet_config();
                (node, chain_spec)
            }
        };

        if is_verbose {
            node = node.informant_enable_color(true);
        }

        node = node
            .role(Role::Authority)
            .impl_version(format!("{}-{}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH")))
            .impl_name("Subspace CLI".to_string());

        crate::utils::apply_extra_options(&node.configuration(), extra)
            .context("Failed to deserialize node config")?
            .build(
                directory,
                chain_spec
            )
            .await
            .into_eyre()
            .wrap_err("Failed to build subspace node")
    }
}

/// Advanced Farmer Settings Wrapper for CLI
#[derive(Deserialize, Serialize, Clone, Derivative, Debug, PartialEq)]
#[derivative(Default)]
pub(crate) struct AdvancedFarmerSettings {
    #[serde(default, skip_serializing_if = "crate::utils::is_default")]
    //TODO: change this back to 1GB when DSN is working properly
    #[derivative(Default(value = "subspace_sdk::ByteSize::gb(3)"))]
    pub(crate) cache_size: ByteSize,
    #[serde(default, flatten)]
    pub(crate) extra: toml::Table,
}

/// Farmer Options Wrapper for CLI
#[derive(Deserialize, Serialize, Clone, Debug)]
pub(crate) struct FarmerConfig {
    pub(crate) reward_address: PublicKey,
    pub(crate) farm_directory: PathBuf,
    pub(crate) farm_size: ByteSize,
    #[serde(default, skip_serializing_if = "crate::utils::is_default")]
    pub(crate) advanced: AdvancedFarmerSettings,
}

impl FarmerConfig {
    pub async fn build(self, node: &Node) -> Result<Farmer> {
        let farm_description = &[FarmDescription::new(self.farm_directory, self.farm_size)];

        // currently we do not have different configuration for the farmer w.r.t
        // different chains, but we may in the future
        let farmer = Farmer::builder();
        crate::utils::apply_extra_options(&farmer.configuration(), self.advanced.extra)
            .context("Failed to deserialize node config")?
            .build(
                self.reward_address,
                node,
                farm_description,
                // TODO: Make this configurable via user input
                NonZeroU8::new(1).expect("static value should not fail; qed"),
            )
            .await
            .context("Failed to build a farmer")
    }
}

/// Enum for Chain
#[derive(Deserialize, Serialize, Default, Clone, Debug, EnumIter)]
pub(crate) enum ChainConfig {
    #[default]
    Gemini3g,
    Dev,
    DevNet,
}

impl std::str::FromStr for ChainConfig {
    type Err = Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gemini3g" => Ok(ChainConfig::Gemini3g),
            "dev" => Ok(ChainConfig::Dev),
            "devnet" => Ok(ChainConfig::DevNet),
            _ => Err(eyre!("given chain: `{s}` is not recognized!")),
        }
    }
}

/// Creates a config file at the location
/// - **Linux:** `$HOME/.config/pulsar/settings.toml`.
/// - **macOS:** `$HOME/Library/Application Support/pulsar/settings.toml`.
/// - **Windows:** `{FOLDERID_RoamingAppData}/pulsar/settings.toml`.
pub(crate) fn create_config() -> Result<(File, PathBuf)> {
    let config_path =
        dirs::config_dir().expect("couldn't get the default config directory!").join("pulsar");

    if let Err(err) = create_dir_all(&config_path) {
        let config_path = config_path.to_str().expect("couldn't get pulsar config path!");
        return Err(err).wrap_err(format!("could not create the directory: `{config_path}`"));
    }

    let file = File::create(config_path.join("settings.toml"))?;

    Ok((file, config_path))
}

/// parses the config, and returns [`Config`]
#[instrument]
pub(crate) fn parse_config() -> Result<Config> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    let config_path = config_path.join("pulsar").join("settings.toml");

    let config: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;
    Ok(config)
}

/// validates the config for farming
#[instrument]
pub(crate) fn validate_config() -> Result<Config> {
    let config = parse_config()?;

    // validity checks
    if config.farmer.farm_size < MIN_FARM_SIZE {
        return Err(eyre!("farm size should be bigger than {MIN_FARM_SIZE}!"));
    }

    Ok(config)
}

/// deletes the config file
#[instrument]
pub(crate) fn delete_config() -> Result<()> {
    let config_path = dirs::config_dir().expect("couldn't get the default config directory!");
    remove_file(config_path).context("couldn't delete config file")
}
