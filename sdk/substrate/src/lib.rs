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

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;

use derivative::Derivative;
use derive_builder::Builder;
use sc_executor::{WasmExecutionMethod, WasmtimeInstantiationStrategy};
use sc_network::config::{NodeKeyConfig, Secret};
use sc_service::config::{KeystoreConfig, NetworkConfiguration, TransportConfig};
use sc_service::{BasePath, Configuration, DatabaseSource, TracingReceiver};
use sdk_utils::{Multiaddr, MultiaddrWithPeerId};
use serde::{Deserialize, Serialize};
pub use types::*;

mod types;

#[doc(hidden)]
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq)]
#[derivative(Default)]
#[builder(pattern = "owned", build_fn(private, name = "_build"), name = "BaseBuilder")]
#[non_exhaustive]
pub struct Base {
    /// Force block authoring
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub force_authoring: bool,
    /// Set node role
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub role: Role,
    /// Blocks pruning options
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub blocks_pruning: BlocksPruning,
    /// State pruning options
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub state_pruning: PruningMode,
    /// Implementation name
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub impl_name: ImplName,
    /// Implementation version
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub impl_version: ImplVersion,
    /// Rpc settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub rpc: Rpc,
    /// Network settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub network: Network,
    /// Offchain worker settings
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub offchain_worker: OffchainWorker,
    /// Enable color for substrate informant
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub informant_enable_color: bool,
    /// Additional telemetry endpoints
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub telemetry: Vec<(Multiaddr, u8)>,
    /// Dev key seed
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub dev_key_seed: Option<String>,
}

#[doc(hidden)]
#[macro_export]
macro_rules! derive_base {
    (
        $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)? @ $base:ty => $builder:ident {
            $(
                #[doc = $doc:literal]
                $field:ident : $field_ty:ty
            ),+
            $(,)?
        }
    ) => {
        impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? $builder $(< $($lt),+ >)?  {
            $(
            #[doc = $doc]
            pub fn $field(mut self, $field: impl Into<$field_ty>) -> Self {
                self.base = self.base.$field($field.into());
                self
            }
            )*
        }
    };

    ( $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)? @ $base:ty => $builder:ident ) => {
        $crate::derive_base!(
            $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? @ $base => $builder {
            /// Force block authoring
            force_authoring: bool,
            /// Set node role
            role: $crate::Role,
            /// Blocks pruning options
            blocks_pruning: $crate::BlocksPruning,
            /// State pruning options
            state_pruning: $crate::PruningMode,
            /// Implementation name
            impl_name: $crate::ImplName,
            /// Implementation version
            impl_version: $crate::ImplVersion,
            /// Rpc settings
            rpc: $crate::Rpc,
            /// Network settings
            network: $crate::Network,
            /// Offchain worker settings
            offchain_worker: $crate::OffchainWorker,
            /// Enable color for substrate informant
            informant_enable_color: bool,
            /// Additional telemetry endpoints
            telemetry: Vec<(sdk_utils::Multiaddr, u8)>,
            /// Dev key seed
            dev_key_seed: String
        });
    }
}

impl Base {
    const NODE_NAME_MAX_LENGTH: usize = 64;

    pub async fn configuration<CS>(
        self,
        directory: impl AsRef<Path>,
        chain_spec: CS,
    ) -> Configuration
    where
        CS: sc_chain_spec::ChainSpec
            + serde::Serialize
            + serde::de::DeserializeOwned
            + sp_runtime::BuildStorage
            + 'static,
    {
        const NODE_KEY_ED25519_FILE: &str = "secret_ed25519";
        const DEFAULT_NETWORK_CONFIG_PATH: &str = "network";

        let Self {
            force_authoring,
            role,
            blocks_pruning,
            state_pruning,
            impl_name: ImplName(impl_name),
            impl_version: ImplVersion(impl_version),
            rpc:
                Rpc {
                    addr: rpc_addr,
                    port: rpc_port,
                    max_connections: rpc_max_connections,
                    cors: rpc_cors,
                    methods: rpc_methods,
                    max_request_size: rpc_max_request_size,
                    max_response_size: rpc_max_response_size,
                    max_subs_per_conn: rpc_max_subs_per_conn,
                },
            network,
            offchain_worker,
            informant_enable_color,
            telemetry,
            dev_key_seed,
        } = self;

        let base_path = BasePath::new(directory.as_ref());
        let config_dir = base_path.config_dir(chain_spec.id());

        let mut network = {
            let Network {
                listen_addresses,
                boot_nodes,
                force_synced,
                name,
                client_id,
                enable_mdns,
                allow_private_ip,
                allow_non_globals_in_dht,
            } = network;
            let name = name.unwrap_or_else(|| {
                names::Generator::with_naming(names::Name::Numbered)
                    .next()
                    .filter(|name| name.chars().count() < Self::NODE_NAME_MAX_LENGTH)
                    .expect("RNG is available on all supported platforms; qed")
            });

            let client_id = client_id.unwrap_or_else(|| format!("{impl_name}/v{impl_version}"));
            let config_dir = config_dir.join(DEFAULT_NETWORK_CONFIG_PATH);
            let listen_addresses = listen_addresses.into_iter().map(Into::into).collect::<Vec<_>>();

            NetworkConfiguration {
                listen_addresses,
                boot_nodes: chain_spec
                    .boot_nodes()
                    .iter()
                    .cloned()
                    .chain(boot_nodes.into_iter().map(Into::into))
                    .collect(),
                force_synced,
                transport: TransportConfig::Normal { enable_mdns, allow_private_ip },
                allow_non_globals_in_dht,
                ..NetworkConfiguration::new(
                    name,
                    client_id,
                    NodeKeyConfig::Ed25519(Secret::File(config_dir.join(NODE_KEY_ED25519_FILE))),
                    Some(config_dir),
                )
            }
        };

        // Increase default value of 25 to improve success rate of sync
        network.default_peers_set.out_peers = 50;
        // Full + Light clients
        network.default_peers_set.in_peers = 25 + 100;
        let keystore = KeystoreConfig::InMemory;

        // HACK: Tricky way to add extra endpoints as we can't push into telemetry
        // endpoints
        let telemetry_endpoints = match chain_spec.telemetry_endpoints() {
            Some(endpoints) => {
                let Ok(serde_json::Value::Array(extra_telemetry)) =
                    serde_json::to_value(&telemetry)
                else {
                    unreachable!("Will always return an array")
                };
                let Ok(serde_json::Value::Array(telemetry)) = serde_json::to_value(endpoints)
                else {
                    unreachable!("Will always return an array")
                };

                serde_json::from_value(serde_json::Value::Array(
                    telemetry.into_iter().chain(extra_telemetry).collect::<Vec<_>>(),
                ))
                .expect("Serialization is always valid")
            }
            None => sc_service::config::TelemetryEndpoints::new(
                telemetry.into_iter().map(|(endpoint, n)| (endpoint.to_string(), n)).collect(),
            )
            .expect("Never returns an error"),
        };

        Configuration {
            impl_name,
            impl_version,
            tokio_handle: tokio::runtime::Handle::current(),
            transaction_pool: Default::default(),
            network,
            keystore,
            database: DatabaseSource::ParityDb { path: config_dir.join("paritydb").join("full") },
            trie_cache_maximum_size: Some(67_108_864),
            state_pruning: Some(state_pruning.into()),
            blocks_pruning: blocks_pruning.into(),
            wasm_method: WasmExecutionMethod::Compiled {
                instantiation_strategy: WasmtimeInstantiationStrategy::PoolingCopyOnWrite,
            },
            wasm_runtime_overrides: None,
            rpc_addr,
            rpc_port: rpc_port.unwrap_or_default(),
            rpc_methods: rpc_methods.into(),
            rpc_max_connections: rpc_max_connections.unwrap_or_default() as u32,
            rpc_cors,
            rpc_max_request_size: rpc_max_request_size.unwrap_or_default() as u32,
            rpc_max_response_size: rpc_max_response_size.unwrap_or_default() as u32,
            rpc_id_provider: None,
            rpc_max_subs_per_conn: rpc_max_subs_per_conn.unwrap_or_default() as u32,
            prometheus_config: None,
            telemetry_endpoints: Some(telemetry_endpoints),
            default_heap_pages: None,
            offchain_worker: offchain_worker.into(),
            force_authoring,
            disable_grandpa: false,
            dev_key_seed,
            tracing_targets: None,
            tracing_receiver: TracingReceiver::Log,
            chain_spec: Box::new(chain_spec),
            max_runtime_instances: 8,
            announce_block: true,
            role: role.into(),
            base_path,
            data_path: config_dir,
            informant_output_format: sc_informant::OutputFormat {
                enable_color: informant_enable_color,
            },
            runtime_cache_size: 2,
        }
    }
}

/// Node RPC builder
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq, Eq)]
#[derivative(Default)]
#[builder(pattern = "owned", build_fn(private, name = "_build"), name = "RpcBuilder")]
#[non_exhaustive]
pub struct Rpc {
    /// Rpc address
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub addr: Option<SocketAddr>,
    /// RPC port
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub port: Option<u16>,
    /// Maximum number of connections for RPC server. `None` if default.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub max_connections: Option<usize>,
    /// CORS settings for HTTP & WS servers. `None` if all origins are
    /// allowed.
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub cors: Option<Vec<String>>,
    /// RPC methods to expose (by default only a safe subset or all of
    /// them).
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub methods: RpcMethods,
    /// Maximum payload of a rpc request
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub max_request_size: Option<usize>,
    /// Maximum payload of a rpc request
    #[builder(setter(strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub max_response_size: Option<usize>,
    /// Maximum allowed subscriptions per rpc connection
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub max_subs_per_conn: Option<usize>,
}

impl RpcBuilder {
    /// Dev configuration
    pub fn dev() -> Self {
        Self::default()
    }

    /// Local test configuration to have rpc exposed locally
    pub fn local_test(port: u16) -> Self {
        Self::dev()
            .addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port))
            .port(port)
            .max_connections(100)
            .max_request_size(10 * 1024)
            .max_response_size(10 * 1024)
            .max_subs_per_conn(Some(100))
    }

    /// Gemini 3g configuration
    pub fn gemini_3g() -> Self {
        Self::new().addr("127.0.0.1:9944".parse().expect("hardcoded value is true")).cors(vec![
            "http://localhost:*".to_owned(),
            "http://127.0.0.1:*".to_owned(),
            "https://localhost:*".to_owned(),
            "https://127.0.0.1:*".to_owned(),
            "https://polkadot.js.org".to_owned(),
        ])
    }

    /// Devnet configuration
    pub fn devnet() -> Self {
        Self::new().addr("127.0.0.1:9944".parse().expect("hardcoded value is true")).cors(vec![
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
#[builder(pattern = "owned", build_fn(private, name = "_build"), name = "NetworkBuilder")]
#[non_exhaustive]
pub struct Network {
    /// Listen on some address for other nodes
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub enable_mdns: bool,
    /// Listen on some address for other nodes
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub allow_private_ip: bool,
    /// Allow non globals in network DHT
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub allow_non_globals_in_dht: bool,
    /// Listen on some address for other nodes
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub listen_addresses: Vec<Multiaddr>,
    /// Boot nodes
    #[builder(default)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boot_nodes: Vec<MultiaddrWithPeerId>,
    /// Force node to think it is synced
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub force_synced: bool,
    /// Node name
    #[builder(setter(into, strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub name: Option<String>,
    /// Client id for telemetry (default is `{IMPL_NAME}/v{IMPL_VERSION}`)
    #[builder(setter(into, strip_option), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub client_id: Option<String>,
}

impl NetworkBuilder {
    /// Dev chain configuration
    pub fn dev() -> Self {
        Self::default().force_synced(true).allow_private_ip(true)
    }

    /// Gemini 3g configuration
    pub fn gemini_3g() -> Self {
        Self::default()
            .listen_addresses(vec![
                "/ip6/::/tcp/30333".parse().expect("hardcoded value is true"),
                "/ip4/0.0.0.0/tcp/30333".parse().expect("hardcoded value is true"),
            ])
            .enable_mdns(true)
    }

    /// Dev network configuration
    pub fn devnet() -> Self {
        Self::default()
            .listen_addresses(vec![
                "/ip6/::/tcp/30333".parse().expect("hardcoded value is true"),
                "/ip4/0.0.0.0/tcp/30333".parse().expect("hardcoded value is true"),
            ])
            .enable_mdns(true)
    }
}

sdk_utils::generate_builder!(Base, Rpc, Network);
