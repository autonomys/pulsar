use derivative::Derivative;
use derive_builder::Builder;
use derive_more::{Deref, DerefMut, Display, From};
use sdk_utils::ByteSize;
use serde::{Deserialize, Serialize};

/// Block pruning settings.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Eq, PartialOrd, Ord)]
pub enum BlocksPruning {
    #[default]
    /// Keep full block history, of every block that was ever imported.
    KeepAll,
    /// Keep full finalized block history.
    KeepFinalized,
    /// Keep N recent finalized blocks.
    Some(u32),
}

impl From<sc_service::BlocksPruning> for BlocksPruning {
    fn from(value: sc_service::BlocksPruning) -> Self {
        match value {
            sc_service::BlocksPruning::KeepAll => Self::KeepAll,
            sc_service::BlocksPruning::KeepFinalized => Self::KeepFinalized,
            sc_service::BlocksPruning::Some(n) => Self::Some(n),
        }
    }
}

impl From<BlocksPruning> for sc_service::BlocksPruning {
    fn from(value: BlocksPruning) -> Self {
        match value {
            BlocksPruning::KeepAll => Self::KeepAll,
            BlocksPruning::KeepFinalized => Self::KeepFinalized,
            BlocksPruning::Some(n) => Self::Some(n),
        }
    }
}

/// Pruning constraints. If none are specified pruning is
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Constraints {
    /// Maximum blocks. Defaults to 0 when unspecified, effectively keeping
    /// only non-canonical states.
    pub max_blocks: Option<u32>,
}

impl From<Constraints> for sc_state_db::Constraints {
    fn from(Constraints { max_blocks }: Constraints) -> Self {
        Self { max_blocks }
    }
}

impl From<sc_state_db::Constraints> for Constraints {
    fn from(sc_state_db::Constraints { max_blocks }: sc_state_db::Constraints) -> Self {
        Self { max_blocks }
    }
}

/// Pruning mode.
#[derive(Debug, Clone, Eq, PartialEq, Default, Serialize, Deserialize)]
pub enum PruningMode {
    /// No pruning. Canonicalization is a no-op.
    #[default]
    ArchiveAll,
    /// Canonicalization discards non-canonical nodes. All the canonical
    /// nodes are kept in the DB.
    ArchiveCanonical,
    /// Maintain a pruning window.
    Constrained(Constraints),
}

impl From<PruningMode> for sc_service::PruningMode {
    fn from(value: PruningMode) -> Self {
        match value {
            PruningMode::ArchiveAll => Self::ArchiveAll,
            PruningMode::ArchiveCanonical => Self::ArchiveCanonical,
            PruningMode::Constrained(c) => Self::Constrained(c.into()),
        }
    }
}

impl From<sc_service::PruningMode> for PruningMode {
    fn from(value: sc_service::PruningMode) -> Self {
        match value {
            sc_service::PruningMode::ArchiveAll => Self::ArchiveAll,
            sc_service::PruningMode::ArchiveCanonical => Self::ArchiveCanonical,
            sc_service::PruningMode::Constrained(c) => Self::Constrained(c.into()),
        }
    }
}

/// Type wrapper with default value for implementation name
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct ImplName(
    #[derivative(Default(value = "env!(\"CARGO_PKG_NAME\").to_owned()"))] pub String,
);

/// Type wrapper with default value for implementation version
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct ImplVersion(
    #[derivative(Default(
        value = "format!(\"{}-{}\", env!(\"CARGO_PKG_VERSION\"), env!(\"GIT_HASH\"))"
    ))]
    pub String,
);

/// Storage monitor
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct StorageMonitor {
    /// How much space do we want to reserve
    pub threshold: ByteSize,
    /// Polling period for threshold
    pub polling_period: std::time::Duration,
}

impl From<StorageMonitor> for sc_storage_monitor::StorageMonitorParams {
    fn from(StorageMonitor { threshold, polling_period }: StorageMonitor) -> Self {
        Self {
            threshold: (threshold.as_u64() / bytesize::MIB).max(1),
            polling_period: polling_period.as_secs().max(1) as u32,
        }
    }
}

/// Wrapper with default value for max subscriptions per connection
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct MaxSubsPerConn(#[derivative(Default(value = "1024"))] pub usize);

/// Offchain worker config
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq, Eq)]
#[derivative(Default)]
#[builder(pattern = "owned", build_fn(name = "_build"), name = "OffchainWorkerBuilder")]
#[non_exhaustive]
pub struct OffchainWorker {
    /// Is enabled
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub enabled: bool,
    /// Is indexing enabled
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub indexing_enabled: bool,
}

impl OffchainWorkerBuilder {
    /// Dev chain configuration
    pub fn dev() -> Self {
        Self::default()
    }

    /// Gemini 3g configuration
    pub fn gemini_3g() -> Self {
        Self::default().enabled(true)
    }

    /// Devnet configuration
    pub fn devnet() -> Self {
        Self::default().enabled(true)
    }
}

impl From<OffchainWorker> for sc_service::config::OffchainWorkerConfig {
    fn from(OffchainWorker { enabled, indexing_enabled }: OffchainWorker) -> Self {
        Self { enabled, indexing_enabled }
    }
}

sdk_utils::generate_builder!(OffchainWorker);

/// Role of the local node.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    #[default]
    /// Regular full node.
    Full,
    /// Actual authority.
    Authority,
}

impl From<Role> for sc_service::Role {
    fn from(value: Role) -> Self {
        match value {
            Role::Full => sc_service::Role::Full,
            Role::Authority => sc_service::Role::Authority,
        }
    }
}

/// Available RPC methods.
#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum RpcMethods {
    /// Expose every RPC method only when RPC is listening on `localhost`,
    /// otherwise serve only safe RPC methods.
    #[default]
    Auto,
    /// Allow only a safe subset of RPC methods.
    Safe,
    /// Expose every RPC method (even potentially unsafe ones).
    Unsafe,
}

impl From<RpcMethods> for sc_service::RpcMethods {
    fn from(value: RpcMethods) -> Self {
        match value {
            RpcMethods::Auto => Self::Auto,
            RpcMethods::Safe => Self::Safe,
            RpcMethods::Unsafe => Self::Unsafe,
        }
    }
}
