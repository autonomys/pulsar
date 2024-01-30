use std::num::NonZeroU8;
use std::path::PathBuf;
use std::sync::Arc;

use derive_builder::Builder;
use derive_more::{Deref, DerefMut};
use subspace_sdk::farmer::FarmDescription;
use subspace_sdk::node::{
    chain_spec, ChainSpec, DomainConfigBuilder, DsnBuilder, NetworkBuilder, Role,
};
use subspace_sdk::utils::ByteSize;
use subspace_sdk::MultiaddrWithPeerId;
use tempfile::TempDir;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

pub fn setup() {
    #[cfg(tokio_unstable)]
    let registry = tracing_subscriber::registry().with(console_subscriber::spawn());
    #[cfg(not(tokio_unstable))]
    let registry = tracing_subscriber::registry();

    let _ = registry
        .with(
            tracing_subscriber::fmt::layer().with_test_writer().with_filter(
                "debug,parity-db=info,cranelift_codegen=info,wasmtime_cranelift=info,\
                 subspace_sdk=trace,subspace_farmer=trace,subspace_service=trace,\
                 subspace_farmer::utils::parity_db_store=debug,trie-cache=info,\
                 wasm_overrides=info,jsonrpsee_core=info,libp2p_gossipsub::behaviour=info,\
                 libp2p_core=info,libp2p_tcp=info,multistream_select=info,yamux=info,\
                 libp2p_swarm=info,libp2p_ping=info,subspace_networking::node_runner=info,\
                 subspace_networking::utils::piece_announcement=info,\
                 subspace_farmer::utils::farmer_provider_record_processor=debug,\
                 subspace_farmer::utils::farmer_piece_cache=debug,wasmtime_jit=info,\
                 wasm-runtime=info"
                    .parse::<tracing_subscriber::EnvFilter>()
                    .expect("Env filter directives are correct"),
            ),
        )
        .try_init();
}

#[derive(Builder)]
#[builder(pattern = "immutable", build_fn(private, name = "_build"), name = "NodeBuilder")]
pub struct InnerNode {
    #[builder(default)]
    not_force_synced: bool,
    #[builder(default)]
    boot_nodes: Vec<MultiaddrWithPeerId>,
    #[builder(default)]
    dsn_boot_nodes: Vec<MultiaddrWithPeerId>,
    #[builder(default)]
    not_authority: bool,
    #[builder(default = "chain_spec::dev_config()")]
    chain: ChainSpec,
    #[builder(default = "TempDir::new().map(Arc::new).unwrap()")]
    path: Arc<TempDir>,
    #[cfg(feature = "core-payments")]
    #[builder(default)]
    enable_core: bool,
}

#[derive(Deref, DerefMut)]
pub struct Node {
    #[deref]
    #[deref_mut]
    node: subspace_sdk::Node,
    pub path: Arc<TempDir>,
    pub chain: ChainSpec,
}

impl NodeBuilder {
    pub async fn build(self, enable_domains: bool) -> Node {
        let InnerNode {
            not_force_synced,
            boot_nodes,
            dsn_boot_nodes,
            not_authority,
            chain,
            path,
            #[cfg(feature = "core-payments")]
            enable_core,
        } = self._build().expect("Infallible");
        let node = subspace_sdk::Node::dev()
            .dsn(
                DsnBuilder::dev()
                    .listen_addresses(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
                    .boot_nodes(dsn_boot_nodes),
            )
            .network(
                NetworkBuilder::dev()
                    .force_synced(!not_force_synced)
                    .listen_addresses(vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()])
                    .boot_nodes(boot_nodes),
            )
            .role(if not_authority { Role::Full } else { Role::Authority })
            .is_timekeeper(!not_authority);

        let node = if enable_domains {
            node.domain(Some(DomainConfigBuilder::dev().configuration()))
        } else {
            node
        };

        #[cfg(all(feature = "core-payments", feature = "executor"))]
        let node = if enable_core {
            node.system_domain(subspace_sdk::node::domains::ConfigBuilder::new().core_payments(
                subspace_sdk::node::domains::core_payments::ConfigBuilder::new().build(),
            ))
        } else {
            node
        };

        let node = node.build(path.path().join("node"), chain.clone()).await.unwrap();

        Node { node, path, chain }
    }
}

impl Node {
    pub fn dev() -> NodeBuilder {
        NodeBuilder::default()
    }

    pub fn path(&self) -> Arc<TempDir> {
        Arc::clone(&self.path)
    }

    pub async fn close(self) {
        self.node.close().await.unwrap();
    }
}

#[derive(Builder)]
#[builder(pattern = "immutable", build_fn(private, name = "_build"), name = "FarmerBuilder")]
pub struct InnerFarmer {
    #[builder(default)]
    reward_address: subspace_sdk::PublicKey,
    #[builder(default = "50")]
    pieces_in_sector: u16,
}

#[derive(Deref, DerefMut)]
pub struct Farmer {
    #[deref]
    #[deref_mut]
    farmer: subspace_sdk::Farmer,
    pub path: Arc<TempDir>,
}

impl FarmerBuilder {
    pub async fn build(self, node: &Node, space_pledged: ByteSize) -> Farmer {
        let InnerFarmer { reward_address, pieces_in_sector } = self._build().expect("Infallible");
        let farmer = subspace_sdk::Farmer::builder()
            .max_pieces_in_sector(Some(pieces_in_sector))
            .build(
                reward_address,
                &**node,
                &[FarmDescription::new(
                    node.path().path().join("plot"),
                    // TODO: account for overhead here
                    space_pledged,
                )],
                NonZeroU8::new(20).expect("Static value should not fail; qed"),
            )
            .await
            .unwrap();
        Farmer { farmer, path: node.path() }
    }
}

impl Farmer {
    pub fn dev() -> FarmerBuilder {
        FarmerBuilder::default()
    }

    pub fn plot_dir(&self) -> PathBuf {
        self.path.path().join("plot")
    }

    pub async fn close(self) {
        self.farmer.close().await.unwrap()
    }
}
