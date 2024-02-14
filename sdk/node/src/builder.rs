use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use derivative::Derivative;
use derive_builder::Builder;
use derive_more::{Deref, DerefMut, Display, From};
use sdk_dsn::{Dsn, DsnBuilder};
use sdk_substrate::{
    BlocksPruning, ConsensusChainConfiguration, ConsensusChainConfigurationBuilder, NetworkBuilder,
    PruningMode, RpcBuilder, StorageMonitor,
};
use sdk_utils::{BuilderError, ByteSize};
use serde::{Deserialize, Serialize};

use super::{ChainSpec, Farmer, Node};
use crate::domains::builder::DomainConfig;

/// Wrapper with default value for piece cache size
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
/// Size of cache of pieces that node produces
/// TODO: Set it to 1 GB once DSN is fixed
pub struct PieceCacheSize(#[derivative(Default(value = "ByteSize::gib(3)"))] pub(crate) ByteSize);

/// Wrapper with default value for segment publish concurrent jobs
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct SegmentPublishConcurrency(
    #[derivative(Default(value = "NonZeroUsize::new(10).expect(\"10 > 0\")"))]
    pub(crate)  NonZeroUsize,
);

/// Node builder
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq)]
#[derivative(Default(bound = ""))]
#[builder(
    pattern = "owned",
    build_fn(private, name = "_build", error = "sdk_utils::BuilderError"),
    name = "Builder"
)]
#[non_exhaustive]
pub struct Config<F: Farmer> {
    /// Max number of segments that can be published concurrently, impacts
    /// RAM usage and network bandwidth.
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub segment_publish_concurrency: SegmentPublishConcurrency,
    /// Should we sync blocks from the DSN?
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub sync_from_dsn: bool,
    #[doc(hidden)]
    #[builder(setter(into))]
    #[serde(flatten, skip_serializing_if = "sdk_utils::is_default")]
    pub base: ConsensusChainConfiguration,
    /// DSN settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub dsn: Dsn,
    /// Storage monitor settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub storage_monitor: Option<StorageMonitor>,
    #[builder(setter(skip), default)]
    #[serde(skip, default)]
    _farmer: std::marker::PhantomData<F>,
    /// Optional domain configuration
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub domain: Option<DomainConfig>,
    /// Flag indicating if the node is authority for Proof of time consensus
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub is_timekeeper: bool,
    /// CPU cores that timekeeper can use
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub timekeeper_cpu_cores: HashSet<usize>,
    /// Proof of time entropy
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub pot_external_entropy: Option<String>,
}

impl<F: Farmer + 'static> Config<F> {
    /// Dev configuraiton
    pub fn dev(base_path: PathBuf) -> Result<Builder<F>, BuilderError> {
        Builder::dev(base_path)
    }

    /// Gemini 3g configuraiton
    pub fn gemini_3h(node_name: String, base_path: PathBuf) -> Result<Builder<F>, BuilderError> {
        Builder::gemini_3h(node_name, base_path)
    }

    /// Devnet configuraiton
    pub fn devnet(node_name: String, base_path: PathBuf) -> Result<Builder<F>, BuilderError> {
        Builder::devnet(node_name, base_path)
    }
}

impl<F: Farmer + 'static> Builder<F> {
    /// Dev chain configuration
    pub fn dev(base_path: PathBuf) -> Result<Builder<F>, BuilderError> {
        Ok(Self::new().sync_from_dsn(true).dsn(DsnBuilder::dev().build()?).base(
            ConsensusChainConfigurationBuilder::default()
                .dev(true)
                .base_path(base_path)
                .network(NetworkBuilder::dev().build()?)
                .rpc(RpcBuilder::dev().build()?)
                .state_pruning(PruningMode::ArchiveCanonical)
                .blocks_pruning(BlocksPruning::Number(256))
                .build()?,
        ))
    }

    /// Gemini 3g configuration
    pub fn gemini_3h(node_name: String, base_path: PathBuf) -> Result<Builder<F>, BuilderError> {
        Ok(Self::new().sync_from_dsn(true).dsn(DsnBuilder::gemini_3h().build()?).base(
            ConsensusChainConfigurationBuilder::default()
                .chain("gemini-3h".to_string())
                .name(node_name)
                .farmer(true)
                .base_path(base_path)
                .network(NetworkBuilder::gemini_3h().build()?)
                .rpc(RpcBuilder::gemini_3h().build()?)
                .state_pruning(PruningMode::ArchiveCanonical)
                .blocks_pruning(BlocksPruning::Number(256))
                .build()?,
        ))
    }

    /// Devnet chain configuration
    pub fn devnet(node_name: String, base_path: PathBuf) -> Result<Builder<F>, BuilderError> {
        Ok(Self::new().sync_from_dsn(true).dsn(DsnBuilder::devnet().build()?).base(
            ConsensusChainConfigurationBuilder::default()
                .chain("devnet".to_string())
                .name(node_name)
                .farmer(true)
                .base_path(base_path)
                .network(NetworkBuilder::devnet().build()?)
                .rpc(RpcBuilder::devnet().build()?)
                .state_pruning(PruningMode::ArchiveCanonical)
                .blocks_pruning(BlocksPruning::Number(256))
                .build()?,
        ))
    }

    /// Get configuration for saving on disk
    pub fn configuration(self) -> Result<Config<F>, BuilderError> {
        self._build()
    }

    /// New builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a node with supplied parameters
    async fn build<CSF>(self, chain_spec_fn: CSF) -> anyhow::Result<Node<F>>
    where
        CSF: Fn(String) -> Result<ChainSpec, String>,
    {
        self.configuration()?.build(chain_spec_fn).await
    }
}
