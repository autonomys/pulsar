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
use sc_network::config::{NodeKeyConfig, NonReservedPeerMode, Secret, SetConfig};
use sc_service::{BasePath, Configuration};
use sdk_utils::{Multiaddr, MultiaddrWithPeerId};
use serde::{Deserialize, Serialize};
use subspace_service::config::{
    SubstrateConfiguration, SubstrateNetworkConfiguration, SubstrateRpcConfiguration,
};
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
    /// Enable color for substrate informant
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub informant_enable_color: bool,
    /// Additional telemetry endpoints
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub telemetry: Vec<(Multiaddr, u8)>,
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
            /// Enable color for substrate informant
            informant_enable_color: bool,
            /// Additional telemetry endpoints
            telemetry: Vec<(sdk_utils::Multiaddr, u8)>,
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
        CS: sc_chain_spec::ChainSpec + sp_runtime::BuildStorage + 'static,
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
                    max_subs_per_conn: rpc_max_subs_per_conn,
                },
            network,
            informant_enable_color,
            telemetry,
        } = self;

        let base_path = BasePath::new(directory.as_ref());
        let config_dir = base_path.config_dir(chain_spec.id());

        let network = {
            let Network { listen_addresses, boot_nodes, force_synced, name, allow_private_ip } =
                network;
            let name = name.unwrap_or_else(|| {
                names::Generator::with_naming(names::Name::Numbered)
                    .next()
                    .filter(|name| name.chars().count() < Self::NODE_NAME_MAX_LENGTH)
                    .expect("RNG is available on all supported platforms; qed")
            });

            let config_dir = config_dir.join(DEFAULT_NETWORK_CONFIG_PATH);
            let listen_addresses = listen_addresses.into_iter().map(Into::into).collect::<Vec<_>>();

            SubstrateNetworkConfiguration {
                listen_on: listen_addresses,
                bootstrap_nodes: chain_spec
                    .boot_nodes()
                    .iter()
                    .cloned()
                    .chain(boot_nodes.into_iter().map(Into::into))
                    .collect(),
                node_key: NodeKeyConfig::Ed25519(Secret::File(
                    config_dir.join(NODE_KEY_ED25519_FILE),
                )),
                default_peers_set: SetConfig {
                    in_peers: 125,
                    out_peers: 50,
                    reserved_nodes: vec![],
                    non_reserved_mode: NonReservedPeerMode::Accept,
                },
                node_name: name,
                allow_private_ips: allow_private_ip,
                force_synced,
                public_addresses: vec![],
            }
        };

        let telemetry_endpoints = match chain_spec.telemetry_endpoints() {
            Some(endpoints) => endpoints.clone(),
            None => sc_service::config::TelemetryEndpoints::new(
                telemetry.into_iter().map(|(endpoint, n)| (endpoint.to_string(), n)).collect(),
            )
            .expect("Never returns an error"),
        };

        SubstrateConfiguration {
            impl_name,
            impl_version,
            transaction_pool: Default::default(),
            network,
            state_pruning: Some(state_pruning.into()),
            blocks_pruning: blocks_pruning.into(),
            rpc_options: SubstrateRpcConfiguration {
                listen_on: rpc_addr.unwrap_or(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    rpc_port.unwrap_or(9944),
                )),
                max_connections: rpc_max_connections.unwrap_or(100),
                cors: rpc_cors,
                methods: rpc_methods.into(),
                max_subscriptions_per_connection: rpc_max_subs_per_conn.unwrap_or(100),
            },
            prometheus_listen_on: None,
            telemetry_endpoints: Some(telemetry_endpoints),
            force_authoring,
            chain_spec: Box::new(chain_spec),
            base_path: base_path.path().to_path_buf(),
            informant_output_format: sc_informant::OutputFormat {
                enable_color: informant_enable_color,
            },
            farmer: role == Role::Authority,
        }
        .into()
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
    pub max_connections: Option<u32>,
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
    /// Maximum allowed subscriptions per rpc connection
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub max_subs_per_conn: Option<u32>,
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
            .max_subs_per_conn(Some(100))
    }

    /// Gemini 3g configuration
    pub fn gemini_3h() -> Self {
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
    pub allow_private_ip: bool,
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
}

impl NetworkBuilder {
    /// Dev chain configuration
    pub fn dev() -> Self {
        Self::default().force_synced(true).allow_private_ip(true)
    }

    /// Gemini 3g configuration
    pub fn gemini_3h() -> Self {
        Self::default().listen_addresses(vec![
            "/ip6/::/tcp/30333".parse().expect("hardcoded value is true"),
            "/ip4/0.0.0.0/tcp/30333".parse().expect("hardcoded value is true"),
        ])
    }

    /// Dev network configuration
    pub fn devnet() -> Self {
        Self::default().listen_addresses(vec![
            "/ip6/::/tcp/30333".parse().expect("hardcoded value is true"),
            "/ip4/0.0.0.0/tcp/30333".parse().expect("hardcoded value is true"),
        ])
    }
}

sdk_utils::generate_builder!(Base, Rpc, Network);
