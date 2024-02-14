//! Crate with abstraction over substrate logic

#![warn(
    missing_docs,
    clippy::dbg_macro,
    clippy::unwrap_used,
    clippy::disallowed_types,
    unused_features
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![feature(concat_idents)]

use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use derivative::Derivative;
use derive_builder::Builder;
use evm_domain_runtime::RuntimeGenesisConfig as EvmRuntimeGenesisConfig;
use sc_chain_spec::{ChainType, GenericChainSpec, Properties};
use sc_informant::OutputFormat;
use sc_keystore::{Keystore, LocalKeystore};
use sc_network::config::{NodeKeyConfig, NonReservedPeerMode, Secret, SetConfig, TransportConfig};
use sc_service::config::{KeystoreConfig, TelemetryEndpoints};
use sc_service::{Configuration, TransactionPoolOptions};
use sc_storage_monitor::StorageMonitorParams;
use sdk_utils::{BuilderError, MultiaddrWithPeerId};
use serde::{Deserialize, Serialize};
use sp_core::crypto::{ExposeSecret, SecretString};
use sp_core::sr25519::Pair;
use sp_core::Pair as PairT;
use sp_domains::{DomainId, OperatorId, KEY_TYPE};
use subspace_service::config::{
    SubstrateConfiguration as ConsensusChainSubstrateConfiguration, SubstrateNetworkConfiguration,
    SubstrateRpcConfiguration,
};
pub use types::*;

mod types;

#[doc(hidden)]
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq)]
#[derivative(Default)]
#[builder(
    pattern = "owned",
    build_fn(validate = "Self::validate", error = "sdk_utils::BuilderError")
)]
#[non_exhaustive]
pub struct KeystoreOptions {
    /// Password used by the keystore.
    ///
    /// This allows appending an extra user-defined secret to the seed.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub password: Option<String>,

    /// File that contains the password used by the keystore.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub password_filename: Option<PathBuf>,
}

impl KeystoreOptionsBuilder {
    fn validate(&self) -> Result<(), BuilderError> {
        match (&self.password, &self.password_filename) {
            (None, None) => Err(BuilderError::ValidationError(
                "At least one key store option must be set".to_string(),
            )),
            (Some(_), None) | (None, Some(_)) => Ok(()),
            _ => Err(BuilderError::ValidationError(
                "Only one key store option must be set".to_string(),
            )),
        }
    }
}

impl KeystoreOptions {
    pub fn keystore_config(&self, config_dir: &Path) -> anyhow::Result<KeystoreConfig> {
        const DEFAULT_KEYSTORE_CONFIG_PATH: &str = "keystore";

        let password = if let Some(ref file) = self.password_filename {
            let password = fs::read_to_string(file)
                .map_err(|e| anyhow!("Error while reading password file: {}", e))?;
            Some(SecretString::new(password))
        } else {
            self.password.as_ref().map(|password_string| SecretString::new(password_string.clone()))
        };

        let path = config_dir.join(DEFAULT_KEYSTORE_CONFIG_PATH);

        Ok(KeystoreConfig::Path { path, password })
    }
}

#[doc(hidden)]
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq)]
#[derivative(Default)]
#[builder(pattern = "owned", build_fn(error = "sdk_utils::BuilderError"))]
#[non_exhaustive]
pub struct DomainChainConfiguration {
    /// Rpc settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub rpc: Rpc<{ RPC_DEFAULT_PORT_FOR_DOMAIN }>,

    /// IP and port (TCP) to start Prometheus exporter on
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub prometheus_listen_on: Option<SocketAddr>,

    /// Blocks pruning options
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub blocks_pruning: BlocksPruning,
    /// State pruning options
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub state_pruning: PruningMode,

    /// Network settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub network: Network,

    /// Operator secret key URI to insert into keystore.
    ///
    /// Example: "//Alice".
    ///
    /// If the value is a file, the file content is used as URI.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    keystore_suri: Option<String>,

    /// Keystore options
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    keystore_options: KeystoreOptions,

    /// Maximum number of transactions in the transaction pool
    #[builder(default = "8192")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub pool_limit: usize,
    /// Maximum number of kilobytes of all transactions stored in the pool.
    #[builder(default = "20480")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub pool_kbytes: usize,
    /// How long a transaction is banned for.
    ///
    /// If it is considered invalid. Defaults to 1800s.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub tx_ban_seconds: Option<u64>,
}

impl DomainChainConfiguration {
    fn derive_keypair(
        suri: &SecretString,
        password: &Option<SecretString>,
    ) -> anyhow::Result<Pair> {
        let keypair_result = Pair::from_string(
            suri.expose_secret(),
            password.as_ref().map(|password| password.expose_secret().as_str()),
        );

        keypair_result.map_err(|err| anyhow!("Invalid password {:?}", err))
    }

    fn store_key_in_keystore(
        keystore_path: PathBuf,
        suri: &SecretString,
        password: Option<SecretString>,
    ) -> anyhow::Result<()> {
        let keypair = Self::derive_keypair(suri, &password)?;

        LocalKeystore::open(keystore_path, password)?
            .insert(KEY_TYPE, suri.expose_secret(), &keypair.public())
            .map_err(|()| anyhow!("Failed to insert key into keystore"))
    }

    fn transaction_pool(&self, is_dev: bool) -> TransactionPoolOptions {
        let mut opts = TransactionPoolOptions::default();

        // ready queue
        opts.ready.count = self.pool_limit;
        opts.ready.total_bytes = self.pool_kbytes * 1024;

        // future queue
        let factor = 10;
        opts.future.count = self.pool_limit / factor;
        opts.future.total_bytes = self.pool_kbytes * 1024 / factor;

        opts.ban_time = if let Some(ban_seconds) = self.tx_ban_seconds {
            std::time::Duration::from_secs(ban_seconds)
        } else if is_dev {
            std::time::Duration::from_secs(0)
        } else {
            std::time::Duration::from_secs(30 * 60)
        };

        opts
    }

    pub async fn configuration<CS, CSF>(
        &self,
        maybe_domain_id: Option<DomainId>,
        mut operator_id: Option<OperatorId>,
        dev: bool,
        consensus_chain_configuration: &sc_service::Configuration,
        enable_color: bool,
    ) -> anyhow::Result<(Configuration, DomainId, Option<OperatorId>)> {
        let transaction_pool = self.transaction_pool(dev);

        let DomainChainConfiguration {
            rpc,
            prometheus_listen_on,
            blocks_pruning,
            state_pruning,
            network,
            keystore_suri,
            keystore_options,
            pool_limit: _,
            pool_kbytes: _,
            tx_ban_seconds: _,
        } = self;

        let mut keystore_suri = keystore_suri.clone();
        let mut rpc = rpc.clone();

        let domain_id;
        // Development mode handling is limited to this section
        {
            if dev {
                if operator_id.is_none() {
                    operator_id.replace(OperatorId::default());
                }
                if keystore_suri.is_none() {
                    keystore_suri.replace("//Alice".to_string());
                }
            }

            domain_id = match maybe_domain_id {
                Some(domain_id) => domain_id,
                None =>
                    if dev {
                        DomainId::default()
                    } else {
                        return Err(anyhow!("Domain ID must be provided unless --dev mode is \
                                            used"
                            .to_string(),));
                    },
            };
            rpc.rpc_cors = if rpc.rpc_cors.is_none() {
                if dev {
                    None
                } else {
                    Some(vec![
                        "http://localhost:*".into(),
                        "http://127.0.0.1:*".into(),
                        "https://localhost:*".into(),
                        "https://127.0.0.1:*".into(),
                        "https://polkadot.js.org".into(),
                    ])
                }
            } else {
                rpc.rpc_cors
            };
        }

        // TODO: Create chain spec

        let base_path = consensus_chain_configuration
            .base_path
            .path()
            .join("domains")
            .join(domain_id.to_string());

        let keystore = {
            let keystore_config = keystore_options.keystore_config(&base_path)?;

            if let Some(keystore_suri) = keystore_suri {
                let (path, password) = match &keystore_config {
                    KeystoreConfig::Path { path, password, .. } => (path.clone(), password.clone()),
                    KeystoreConfig::InMemory => {
                        unreachable!("Just constructed non-memory keystore config; qed");
                    }
                };

                let keystore_suri_secret = SecretString::new(keystore_suri.clone());
                Self::store_key_in_keystore(path, &keystore_suri_secret, password)?;
            }

            keystore_config
        };

        // Derive domain chain spec from consensus chain spec
        // TODO: Migrate once https://github.com/paritytech/polkadot-sdk/issues/2963 is un-broken
        #[allow(deprecated)]
        let chain_spec = GenericChainSpec::<EvmRuntimeGenesisConfig>::from_genesis(
            // Name
            &format!("{} Domain {}", consensus_chain_configuration.chain_spec.name(), domain_id),
            // ID
            &format!("{}_domain_{}", consensus_chain_configuration.chain_spec.id(), domain_id),
            ChainType::Custom("SubspaceDomain".to_string()),
            // The value of the `EvmRuntimeGenesisConfig` doesn't matter since genesis storage will
            // be replaced before actually running the domain
            EvmRuntimeGenesisConfig::default,
            // Bootnodes
            consensus_chain_configuration
                .chain_spec
                .properties()
                .get("domainsBootstrapNodes")
                .map(|d| {
                    serde_json::from_value::<
                        HashMap<DomainId, Vec<sc_network::config::MultiaddrWithPeerId>>,
                    >(d.clone())
                })
                .transpose()
                .map_err(|error| {
                    sc_service::Error::Other(format!(
                        "Failed to decode Domains bootstrap nodes: {error:?}"
                    ))
                })?
                .unwrap_or_default()
                .get(&domain_id)
                .cloned()
                .unwrap_or_default(),
            // Telemetry
            None,
            // Protocol ID
            Some(&format!(
                "{}-domain-{}",
                consensus_chain_configuration.chain_spec.id(),
                domain_id
            )),
            None,
            // Properties
            Some({
                let mut properties = Properties::new();

                if let Some(ss58_format) =
                    consensus_chain_configuration.chain_spec.properties().get("ss58Format")
                {
                    properties.insert("ss58Format".to_string(), ss58_format.clone());
                }
                if let Some(decimal_places) =
                    consensus_chain_configuration.chain_spec.properties().get("tokenDecimals")
                {
                    properties.insert("tokenDecimals".to_string(), decimal_places.clone());
                }
                if let Some(token_symbol) =
                    consensus_chain_configuration.chain_spec.properties().get("tokenSymbol")
                {
                    properties.insert("tokenSymbol".to_string(), token_symbol.clone());
                }

                properties
            }),
            // Extensions
            None,
            // Code doesn't matter, it will be replaced before running
            &[],
        );

        let listen_addresses =
            network.listen_on.clone().into_iter().map(Into::into).collect::<Vec<_>>();
        let reserved_nodes =
            network.reserved_nodes.clone().into_iter().map(Into::into).collect::<Vec<_>>();
        let bootstrap_nodes =
            network.bootstrap_nodes.clone().into_iter().map(Into::into).collect::<Vec<_>>();
        let configuration = domain_service::config::SubstrateConfiguration {
            impl_name: consensus_chain_configuration.impl_name.clone(),
            impl_version: consensus_chain_configuration.impl_version.clone(),
            operator: operator_id.is_some(),
            base_path: base_path.clone(),
            transaction_pool,
            network: domain_service::config::SubstrateNetworkConfiguration {
                listen_on: listen_addresses,
                public_addresses: network.public_addr.clone(),
                bootstrap_nodes,
                node_key: consensus_chain_configuration.network.node_key.clone(),
                default_peers_set: SetConfig {
                    in_peers: network.in_peers,
                    out_peers: network.out_peers,
                    reserved_nodes,
                    non_reserved_mode: if network.reserved_only {
                        NonReservedPeerMode::Deny
                    } else {
                        NonReservedPeerMode::Accept
                    },
                },
                node_name: consensus_chain_configuration.network.node_name.clone(),
                allow_private_ips: match consensus_chain_configuration.network.transport {
                    TransportConfig::Normal { allow_private_ip, .. } => allow_private_ip,
                    TransportConfig::MemoryOnly => {
                        unreachable!("Memory transport not used in CLI; qed")
                    }
                },
                force_synced: false,
            },
            keystore,
            state_pruning: Some((*state_pruning).into()),
            blocks_pruning: (*blocks_pruning).into(),
            rpc_options: domain_service::config::SubstrateRpcConfiguration {
                listen_on: rpc.rpc_listen_on,
                max_connections: rpc.rpc_max_connections,
                cors: rpc.rpc_cors,
                methods: match rpc.rpc_methods {
                    RpcMethods::Auto =>
                        if rpc.rpc_listen_on.ip().is_loopback() {
                            sc_service::RpcMethods::Unsafe
                        } else {
                            sc_service::RpcMethods::Safe
                        },
                    RpcMethods::Safe => sc_service::RpcMethods::Safe,
                    RpcMethods::Unsafe => sc_service::RpcMethods::Unsafe,
                },
                max_subscriptions_per_connection: rpc.rpc_max_subscriptions_per_connection,
            },
            prometheus_listen_on: prometheus_listen_on.clone(),
            telemetry_endpoints: consensus_chain_configuration.telemetry_endpoints.clone(),
            force_authoring: false,
            chain_spec: Box::new(chain_spec),
            informant_output_format: OutputFormat { enable_color },
        };

        Ok((configuration.into(), domain_id, operator_id))
    }
}

#[doc(hidden)]
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq)]
#[derivative(Default)]
#[builder(pattern = "owned", build_fn(error = "sdk_utils::BuilderError"))]
#[non_exhaustive]
pub struct ConsensusChainConfiguration {
    /// Enable farmer mode.
    ///
    /// Node will support farmer connections for block and vote production,
    /// implies `--rpc-listen-on 127.0.0.1:9944` unless `--rpc-listen-on` is
    /// specified explicitly.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    farmer: bool,
    /// Base path where to store node files.
    ///
    /// Required unless --dev mode is used.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub base_path: Option<PathBuf>,
    /// Specify the chain specification.
    ///
    /// It can be one of the predefined ones (dev) or it can be a path to a file
    /// with the chainspec (such as one exported by the `build-spec`
    /// subcommand).
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub chain: Option<String>,
    /// Enable development mode.
    ///
    /// Implies following flags (unless customized):
    /// * `--chain dev` (unless specified explicitly)
    /// * `--farmer`
    /// * `--tmp` (unless `--base-path` specified explicitly)
    /// * `--force-synced`
    /// * `--force-authoring`
    /// * `--allow-private-ips`
    /// * `--rpc-cors all` (unless specified explicitly)
    /// * `--dsn-disable-bootstrap-on-start`
    /// * `--timekeeper`
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub dev: bool,
    /// Run a temporary node.
    ///
    /// This will create a temporary directory for storing node data that will
    /// be deleted at the end of the process.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub tmp: bool,
    /// Rpc settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub rpc: Rpc<{ RPC_DEFAULT_PORT_FOR_CONSENSUS }>,
    /// The human-readable name for this node.
    ///
    /// It's used as network node name and in telemetry. Auto-generated if not
    /// specified explicitly.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub name: Option<String>,
    /// Disable connecting to the Substrate telemetry server.
    ///
    /// Telemetry is on by default on global chains.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub no_telemetry: bool,
    /// The URL of the telemetry server to connect to.
    ///
    /// This flag can be passed multiple times as a means to specify multiple
    /// telemetry endpoints. Verbosity levels range from 0-9, with 0 denoting
    /// the least verbosity.
    ///
    /// Expected format is 'URL VERBOSITY', e.g. `--telemetry-url 'wss://foo/bar
    /// 0'`.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub telemetry_endpoints: Vec<(String, u8)>,
    /// IP and port (TCP) to start Prometheus exporter on
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub prometheus_listen_on: Option<SocketAddr>,
    /// Blocks pruning options
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub blocks_pruning: BlocksPruning,
    /// State pruning options
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub state_pruning: PruningMode,
    /// Network settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub network: Network,
    /// Maximum number of transactions in the transaction pool
    #[builder(default = "8192")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub pool_limit: usize,
    /// Maximum number of kilobytes of all transactions stored in the pool.
    #[builder(default = "20480")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub pool_kbytes: usize,
    /// How long a transaction is banned for.
    ///
    /// If it is considered invalid. Defaults to 1800s.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub tx_ban_seconds: Option<u64>,
    /// Parameter that allows node to forcefully assume it is synced, needed for
    /// network bootstrapping only, as long as two synced nodes remain on
    /// the network at any time, this doesn't need to be used.
    ///
    /// --dev mode enables this option automatically.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub force_synced: bool,
    /// Force block authoring
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub force_authoring: bool,
    /// Required available space on database storage.
    ///
    /// If available space for DB storage drops below the given threshold, node
    /// will be gracefully terminated.
    ///
    /// If `0` is given monitoring will be disabled.
    #[builder(setter(name = "db_storage_threshold"), default = "1024")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub threshold: u64,
    /// How often available space is polled.
    #[builder(setter(name = "db_storage_polling_period"), default = "5")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub polling_period: u32,
    /// Implementation name
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub impl_name: ImplName,
    /// Implementation version
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub impl_version: ImplVersion,
    /// Enable color for substrate informant
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub informant_enable_color: bool,
}

impl ConsensusChainConfiguration {
    const NODE_NAME_MAX_LENGTH: usize = 64;

    fn transaction_pool(&self, is_dev: bool) -> TransactionPoolOptions {
        let mut opts = TransactionPoolOptions::default();

        // ready queue
        opts.ready.count = self.pool_limit;
        opts.ready.total_bytes = self.pool_kbytes * 1024;

        // future queue
        let factor = 10;
        opts.future.count = self.pool_limit / factor;
        opts.future.total_bytes = self.pool_kbytes * 1024 / factor;

        opts.ban_time = if let Some(ban_seconds) = self.tx_ban_seconds {
            std::time::Duration::from_secs(ban_seconds)
        } else if is_dev {
            std::time::Duration::from_secs(0)
        } else {
            std::time::Duration::from_secs(30 * 60)
        };

        opts
    }

    pub async fn configuration<CS, CSF>(
        self,
        chain_spec_fn: CSF,
    ) -> anyhow::Result<(bool, StorageMonitorParams, Configuration)>
    where
        CS: sc_chain_spec::ChainSpec + sp_runtime::BuildStorage + 'static,
        CSF: Fn(String) -> Result<CS, String>,
    {
        const NODE_KEY_ED25519_FILE: &str = "secret_ed25519";
        const DEFAULT_NETWORK_CONFIG_PATH: &str = "network";

        let transaction_pool = self.transaction_pool(self.dev);

        let Self {
            mut farmer,
            base_path,
            mut chain,
            dev,
            mut tmp,
            mut rpc,
            name,
            no_telemetry,
            telemetry_endpoints,
            prometheus_listen_on,
            blocks_pruning,
            state_pruning,
            mut network,
            pool_limit: _,
            pool_kbytes: _,
            tx_ban_seconds: _,
            mut force_synced,
            mut force_authoring,
            threshold,
            polling_period,
            impl_name,
            impl_version,
            informant_enable_color,
        } = self;

        {
            if dev {
                if chain.is_none() {
                    chain = Some("dev".to_string());
                }
                farmer = true;
                tmp = true;
                force_synced = true;
                force_authoring = true;
                network.allow_private_ips = true;
            }

            rpc.rpc_cors = if rpc.rpc_cors.is_none() {
                if dev {
                    None
                } else {
                    Some(vec![
                        "http://localhost:*".into(),
                        "http://127.0.0.1:*".into(),
                        "https://localhost:*".into(),
                        "https://127.0.0.1:*".into(),
                        "https://polkadot.js.org".into(),
                    ])
                }
            } else {
                rpc.rpc_cors
            };
        }

        let chain_spec = match chain.as_deref() {
            Some(chain_id) => chain_spec_fn(String::from(chain_id)).map_err(|e| {
                anyhow!("Error in building chain spec for chain id: {:?}, error: {:?}", chain_id, e)
            })?,
            None => {
                return Err(anyhow!(
                    "Chain must be provided unless --dev mode is used".to_string(),
                ));
            }
        };

        let mut maybe_tmp_dir = None;
        let base_path = match base_path {
            Some(base_path) => base_path,
            None =>
                if tmp {
                    let tmp = tempfile::Builder::new().prefix("subspace-node-").tempdir().map_err(
                        |error| {
                            anyhow!(format!(
                                "Failed to create temporary directory for node: {error}"
                            ))
                        },
                    )?;

                    maybe_tmp_dir.insert(tmp).path().to_path_buf()
                } else {
                    return Err(anyhow!("--base-path is required".to_string()));
                },
        };
        let config_dir = base_path.join(chain_spec.id());

        let network = {
            let Network {
                bootstrap_nodes,
                reserved_nodes,
                reserved_only,
                public_addr,
                listen_on,
                allow_private_ips,
                out_peers,
                in_peers,
            } = network;
            let name = name.unwrap_or_else(|| {
                names::Generator::with_naming(names::Name::Numbered)
                    .next()
                    .filter(|name| name.chars().count() < Self::NODE_NAME_MAX_LENGTH)
                    .expect("RNG is available on all supported platforms; qed")
            });

            let network_config_dir = config_dir.join(DEFAULT_NETWORK_CONFIG_PATH);
            let listen_addresses = listen_on.into_iter().map(Into::into).collect::<Vec<_>>();
            let reserved_nodes = reserved_nodes.into_iter().map(Into::into).collect::<Vec<_>>();

            SubstrateNetworkConfiguration {
                listen_on: listen_addresses,
                bootstrap_nodes: chain_spec
                    .boot_nodes()
                    .iter()
                    .cloned()
                    .chain(bootstrap_nodes.into_iter().map(Into::into))
                    .collect(),
                node_key: NodeKeyConfig::Ed25519(Secret::File(
                    network_config_dir.join(NODE_KEY_ED25519_FILE),
                )),
                default_peers_set: SetConfig {
                    in_peers,
                    out_peers,
                    reserved_nodes,
                    non_reserved_mode: if reserved_only {
                        NonReservedPeerMode::Deny
                    } else {
                        NonReservedPeerMode::Accept
                    },
                },
                node_name: name,
                allow_private_ips,
                force_synced,
                public_addresses: public_addr,
            }
        };

        let configuration = ConsensusChainSubstrateConfiguration {
            impl_name: impl_name.to_string(),
            impl_version: impl_version.to_string(),
            transaction_pool,
            network,
            state_pruning: state_pruning.into(),
            blocks_pruning: blocks_pruning.into(),
            rpc_options: SubstrateRpcConfiguration {
                listen_on: rpc.rpc_listen_on,
                max_connections: rpc.rpc_max_connections,
                cors: rpc.rpc_cors,
                methods: match rpc.rpc_methods {
                    RpcMethods::Auto =>
                        if rpc.rpc_listen_on.ip().is_loopback() {
                            sc_service::RpcMethods::Unsafe
                        } else {
                            sc_service::RpcMethods::Safe
                        },
                    RpcMethods::Safe => sc_service::RpcMethods::Safe,
                    RpcMethods::Unsafe => sc_service::RpcMethods::Unsafe,
                },
                max_subscriptions_per_connection: rpc.rpc_max_subscriptions_per_connection,
            },
            prometheus_listen_on,
            telemetry_endpoints: if no_telemetry {
                None
            } else if !telemetry_endpoints.is_empty() {
                Some(TelemetryEndpoints::new(telemetry_endpoints)?)
            } else {
                chain_spec.telemetry_endpoints().clone()
            },
            force_authoring,
            chain_spec: Box::new(chain_spec),
            base_path,
            informant_output_format: sc_informant::OutputFormat {
                enable_color: informant_enable_color,
            },
            farmer,
        }
        .into();

        Ok((dev, StorageMonitorParams { threshold, polling_period }, configuration))
    }
}

/// Node RPC builder
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq, Eq)]
#[derivative(Default)]
#[builder(pattern = "owned", build_fn(error = "sdk_utils::BuilderError"))]
#[non_exhaustive]
pub struct Rpc<const DEFAULT_PORT: u16> {
    /// IP and port (TCP) on which to listen for RPC requests.
    ///
    /// Note: not all RPC methods are safe to be exposed publicly. Use an RPC
    /// proxy server to filter out dangerous methods.
    /// More details: <https://docs.substrate.io/main-docs/build/custom-rpc/#public-rpcs>.
    #[derivative(Default(value = "SocketAddr::new(
    IpAddr::V4(Ipv4Addr::LOCALHOST),
    DEFAULT_PORT,
    )"))]
    #[builder(default = "SocketAddr::new(
    IpAddr::V4(Ipv4Addr::LOCALHOST),
    DEFAULT_PORT,
    )")]
    pub rpc_listen_on: SocketAddr,

    /// RPC methods to expose.
    /// - `unsafe`: Exposes every RPC method.
    /// - `safe`: Exposes only a safe subset of RPC methods, denying unsafe RPC
    ///   methods.
    /// - `auto`: Acts as `safe` if non-localhost `--rpc-listen-on` is passed,
    ///   otherwise acts as `unsafe`.
    #[builder(default = "RpcMethods::Auto")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub rpc_methods: RpcMethods,

    /// Set the the maximum concurrent subscriptions per connection.
    #[builder(default = "RPC_DEFAULT_MAX_SUBS_PER_CONN")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub rpc_max_subscriptions_per_connection: u32,

    /// Maximum number of RPC server connections.
    #[builder(default = "RPC_DEFAULT_MAX_CONNECTIONS")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub rpc_max_connections: u32,

    /// Specify browser Origins allowed to access the HTTP & WS RPC servers.
    /// A comma-separated list of origins (protocol://domain or special `null`
    /// value). Value of `all` will disable origin validation. Default is to
    /// allow localhost and <https://polkadot.js.org> origins. When running in
    /// --dev mode the default is to allow all origins.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub rpc_cors: Option<Vec<String>>,
}

impl<const DEFAULT_PORT: u16> RpcBuilder<DEFAULT_PORT> {
    /// Dev configuration
    pub fn dev() -> Self {
        Self::default()
    }

    /// Local test configuration to have rpc exposed locally
    pub fn local_test() -> Self {
        Self::create_empty().rpc_max_connections(100).rpc_max_subscriptions_per_connection(100)
    }

    /// Gemini 3g configuration
    pub fn gemini_3h() -> Self {
        Self::create_empty()
            .rpc_max_connections(100)
            .rpc_max_subscriptions_per_connection(100)
            .rpc_cors(vec![
                "http://localhost:*".to_owned(),
                "http://127.0.0.1:*".to_owned(),
                "https://localhost:*".to_owned(),
                "https://127.0.0.1:*".to_owned(),
                "https://polkadot.js.org".to_owned(),
            ])
    }

    /// Devnet configuration
    pub fn devnet() -> Self {
        Self::create_empty()
            .rpc_max_connections(100)
            .rpc_max_subscriptions_per_connection(100)
            .rpc_cors(vec![
                "http://localhost:*".to_owned(),
                "http://127.0.0.1:*".to_owned(),
                "https://localhost:*".to_owned(),
                "https://127.0.0.1:*".to_owned(),
                "https://polkadot.js.org".to_owned(),
            ])
    }
}

/// Node network builder
#[derive(Debug, Default, Clone, Builder, Deserialize, Serialize, PartialEq)]
#[builder(pattern = "owned", build_fn(error = "sdk_utils::BuilderError"))]
#[non_exhaustive]
pub struct Network {
    /// Specify a list of bootstrap nodes for Substrate networking stack.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    bootstrap_nodes: Vec<MultiaddrWithPeerId>,

    /// Specify a list of reserved node addresses.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    reserved_nodes: Vec<MultiaddrWithPeerId>,

    /// Whether to only synchronize the chain with reserved nodes.
    ///
    /// TCP connections might still be established with non-reserved nodes.
    /// In particular, if you are a farmer your node might still connect to
    /// other farmer nodes regardless of whether they are defined as
    /// reserved nodes.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    reserved_only: bool,

    /// The public address that other nodes will use to connect to it.
    ///
    /// This can be used if there's a proxy in front of this node or if address
    /// is known beforehand and less reliable auto-discovery can be avoided.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    public_addr: Vec<sc_network::Multiaddr>,

    /// Listen on this multiaddress
    #[builder(default = "vec![
    sc_network::Multiaddr::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    .with(sc_network::multiaddr::Protocol::Tcp(30333)),
    sc_network::Multiaddr::from(IpAddr::V6(Ipv6Addr::UNSPECIFIED))
    .with(sc_network::multiaddr::Protocol::Tcp(30333))
    ]")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    listen_on: Vec<sc_network::Multiaddr>,

    /// Determines whether we allow keeping non-global (private, shared,
    /// loopback..) addresses in Kademlia DHT.
    #[builder(default = "false")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    allow_private_ips: bool,

    /// Specify the number of outgoing connections we're trying to maintain.
    #[builder(default = "8")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    out_peers: u32,

    /// Maximum number of inbound full nodes peers.
    #[builder(default = "32")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    in_peers: u32,
}

impl NetworkBuilder {
    /// Dev chain configuration
    pub fn dev() -> Self {
        Self::default().allow_private_ips(true)
    }

    /// Gemini 3g configuration
    pub fn gemini_3h() -> Self {
        Self::default()
    }

    /// Dev network configuration
    pub fn devnet() -> Self {
        Self::default()
    }
}
