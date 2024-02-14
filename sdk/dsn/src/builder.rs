use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use anyhow::Context;
use derivative::Derivative;
use derive_builder::Builder;
use derive_more::{Deref, DerefMut, Display, From};
use futures::prelude::*;
use prometheus_client::registry::Registry;
use sc_consensus_subspace::archiver::SegmentHeadersStore;
use sdk_utils::{self, DestructorSet, Multiaddr, MultiaddrWithPeerId};
use serde::{Deserialize, Serialize};
use subspace_farmer::piece_cache::PieceCache as FarmerPieceCache;
use subspace_farmer::utils::readers_and_pieces::ReadersAndPieces;
use subspace_farmer::KNOWN_PEERS_CACHE_SIZE;
use subspace_networking::libp2p::multiaddr::{Multiaddr as LibP2PMultiAddress, Protocol};
use subspace_networking::utils::strip_peer_id;
use subspace_networking::{
    KademliaMode, KnownPeersManager, KnownPeersManagerConfig, PieceByIndexRequest,
    PieceByIndexRequestHandler, PieceByIndexResponse, SegmentHeaderBySegmentIndexesRequestHandler,
    SegmentHeaderRequest, SegmentHeaderResponse,
};

use super::local_provider_record_utils::MaybeLocalRecordProvider;
use super::LocalRecordProvider;

/// Wrapper with default value for listen address
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct ListenAddresses(
    #[derivative(Default(value = "vec![
        LibP2PMultiAddress::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(30533))
            .with(Protocol::QuicV1).into(),
        LibP2PMultiAddress::from(IpAddr::V6(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Udp(30533))
            .with(Protocol::QuicV1).into(),
        LibP2PMultiAddress::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(30533)).into(),
        LibP2PMultiAddress::from(IpAddr::V6(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Tcp(30533)).into(),
        LibP2PMultiAddress::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Udp(30433))
            .with(Protocol::QuicV1).into(),
        LibP2PMultiAddress::from(IpAddr::V6(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Udp(30433))
            .with(Protocol::QuicV1).into(),
        LibP2PMultiAddress::from(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .with(Protocol::Tcp(30433)).into(),
        LibP2PMultiAddress::from(IpAddr::V6(Ipv6Addr::UNSPECIFIED))
            .with(Protocol::Tcp(30433)).into()
    ]"))]
    pub Vec<Multiaddr>,
);

/// Wrapper with default value for number of incoming connections
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct InConnections(#[derivative(Default(value = "300"))] pub u32);

/// Wrapper with default value for number of outgoing connections
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct OutConnections(#[derivative(Default(value = "150"))] pub u32);

/// Wrapper with default value for number of target connections
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct TargetConnections(#[derivative(Default(value = "15"))] pub u32);

/// Wrapper with default value for number of pending incoming connections
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct PendingInConnections(#[derivative(Default(value = "100"))] pub u32);

/// Wrapper with default value for number of pending outgoing connections
#[derive(
    Debug, Clone, Derivative, Deserialize, Serialize, PartialEq, Eq, From, Deref, DerefMut, Display,
)]
#[derivative(Default)]
#[serde(transparent)]
pub struct PendingOutConnections(#[derivative(Default(value = "150"))] pub u32);

/// Node DSN builder
#[derive(Debug, Clone, Derivative, Builder, Deserialize, Serialize, PartialEq)]
#[derivative(Default)]
#[builder(pattern = "immutable", build_fn(error = "sdk_utils::BuilderError"))]
#[non_exhaustive]
pub struct Dsn {
    /// Listen on some address for other nodes
    #[builder(default, setter(into))]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub listen_on: ListenAddresses,
    /// Boot nodes
    #[builder(default)]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bootstrap_nodes: Vec<MultiaddrWithPeerId>,
    /// Known external addresses
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub external_addresses: Vec<Multiaddr>,
    /// Reserved peers
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub reserved_peers: Vec<Multiaddr>,
    /// Determines whether we allow keeping non-global (private, shared,
    /// loopback..) addresses in Kademlia DHT.
    #[builder(default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub allow_non_global_addresses_in_dht: bool,
    /// Defines max established incoming swarm connection limit.
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub in_connections: InConnections,
    /// Defines max established outgoing swarm connection limit.
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub out_connections: OutConnections,
    /// Pending incoming swarm connection limit.
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub pending_in_connections: PendingInConnections,
    /// Pending outgoing swarm connection limit.
    #[builder(setter(into), default)]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub pending_out_connections: PendingOutConnections,
    /// Defines whether we should run blocking Kademlia bootstrap() operation
    /// before other requests.
    #[builder(default = "false")]
    #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
    pub disable_bootstrap_on_start: bool,
}

impl DsnBuilder {
    /// Dev chain configuration
    pub fn dev() -> Self {
        Self::default().allow_non_global_addresses_in_dht(true).disable_bootstrap_on_start(true)
    }

    /// Gemini 3g configuration
    pub fn gemini_3h() -> Self {
        Self::default()
    }

    /// Gemini 3g configuration
    pub fn devnet() -> Self {
        Self::default()
    }
}

/// Options for DSN
pub struct DsnOptions<C, PieceByIndex, SegmentHeaderByIndexes> {
    /// Client to aux storage for node piece cache
    pub client: Arc<C>,
    /// Path for dsn
    pub base_path: PathBuf,
    /// Keypair for networking
    pub keypair: subspace_networking::libp2p::identity::Keypair,
    /// Get piece by hash handler
    pub get_piece_by_index: PieceByIndex,
    /// Get segment header by segment indexes handler
    pub get_segment_header_by_segment_indexes: SegmentHeaderByIndexes,
    /// Segment header store
    pub segment_header_store: SegmentHeadersStore<C>,
    /// Is libp2p metrics enabled
    pub is_metrics_enabled: bool,
}

/// Shared Dsn structure between node and farmer
#[derive(Derivative)]
#[derivative(Debug)]
pub struct DsnShared {
    /// Dsn node
    pub node: subspace_networking::Node,
    /// Farmer readers and pieces
    pub farmer_readers_and_pieces: Arc<parking_lot::Mutex<Option<ReadersAndPieces>>>,
    /// Farmer piece cache
    pub farmer_piece_cache: Arc<parking_lot::RwLock<Option<FarmerPieceCache>>>,
    _destructors: DestructorSet,
}

impl Dsn {
    /// Build dsn
    pub fn build_dsn<B, C, PieceByIndex, F1, SegmentHeaderByIndexes>(
        self,
        options: DsnOptions<C, PieceByIndex, SegmentHeaderByIndexes>,
    ) -> anyhow::Result<(
        DsnShared,
        subspace_networking::NodeRunner<LocalRecordProvider>,
        Option<Registry>,
    )>
    where
        B: sp_runtime::traits::Block,
        C: sc_client_api::AuxStore + sp_blockchain::HeaderBackend<B> + Send + Sync + 'static,
        PieceByIndex: Fn(
                &PieceByIndexRequest,
                Weak<parking_lot::Mutex<Option<ReadersAndPieces>>>,
                Arc<parking_lot::RwLock<Option<FarmerPieceCache>>>,
            ) -> F1
            + Send
            + Sync
            + 'static,
        F1: Future<Output = Option<PieceByIndexResponse>> + Send + 'static,
        SegmentHeaderByIndexes: Fn(&SegmentHeaderRequest, &SegmentHeadersStore<C>) -> Option<SegmentHeaderResponse>
            + Send
            + Sync
            + 'static,
    {
        let DsnOptions {
            client,
            base_path,
            keypair,
            get_piece_by_index,
            get_segment_header_by_segment_indexes,
            segment_header_store,
            is_metrics_enabled,
        } = options;
        let farmer_readers_and_pieces = Arc::new(parking_lot::Mutex::new(None));
        let protocol_version = hex::encode(client.info().genesis_hash);
        let farmer_piece_cache = Arc::new(parking_lot::RwLock::new(None));
        let local_records_provider = MaybeLocalRecordProvider::new(farmer_piece_cache.clone());

        let mut metrics_registry = Registry::default();

        tracing::debug!(genesis_hash = protocol_version, "Setting DSN protocol version...");

        let Self {
            listen_on,
            reserved_peers,
            allow_non_global_addresses_in_dht,
            in_connections: InConnections(max_established_incoming_connections),
            out_connections: OutConnections(max_established_outgoing_connections),
            pending_in_connections: PendingInConnections(max_pending_incoming_connections),
            pending_out_connections: PendingOutConnections(max_pending_outgoing_connections),
            bootstrap_nodes,
            external_addresses,
            disable_bootstrap_on_start,
        } = self;

        let bootstrap_nodes = bootstrap_nodes.into_iter().map(Into::into).collect::<Vec<_>>();

        let listen_on = listen_on.0.into_iter().map(Into::into).collect();

        let networking_parameters_registry = KnownPeersManager::new(KnownPeersManagerConfig {
            path: Some(base_path.join("known_addresses.bin").into_boxed_path()),
            ignore_peer_list: strip_peer_id(bootstrap_nodes.clone())
                .into_iter()
                .map(|(peer_id, _)| peer_id)
                .collect::<HashSet<_>>(),
            cache_size: KNOWN_PEERS_CACHE_SIZE,
            ..Default::default()
        })
        .context("Failed to open known addresses database for DSN")?
        .boxed();

        let default_networking_config = subspace_networking::Config::new(
            protocol_version,
            keypair,
            local_records_provider.clone(),
            is_metrics_enabled.then_some(&mut metrics_registry),
        );

        let config = subspace_networking::Config {
            listen_on,
            allow_non_global_addresses_in_dht,
            networking_parameters_registry,
            request_response_protocols: vec![
                PieceByIndexRequestHandler::create({
                    let weak_readers_and_pieces = Arc::downgrade(&farmer_readers_and_pieces);
                    let farmer_piece_cache = farmer_piece_cache.clone();
                    move |_, req| {
                        let weak_readers_and_pieces = weak_readers_and_pieces.clone();
                        let farmer_piece_cache = farmer_piece_cache.clone();

                        get_piece_by_index(req, weak_readers_and_pieces, farmer_piece_cache)
                    }
                }),
                SegmentHeaderBySegmentIndexesRequestHandler::create({
                    let segment_header_store = segment_header_store.clone();
                    move |_, req| {
                        futures::future::ready(get_segment_header_by_segment_indexes(
                            req,
                            &segment_header_store,
                        ))
                    }
                }),
            ],
            reserved_peers: reserved_peers.into_iter().map(Into::into).collect(),
            max_established_incoming_connections,
            max_established_outgoing_connections,
            max_pending_incoming_connections,
            max_pending_outgoing_connections,
            bootstrap_addresses: bootstrap_nodes,
            kademlia_mode: KademliaMode::Dynamic,
            external_addresses: external_addresses.into_iter().map(Into::into).collect(),
            disable_bootstrap_on_start,
            ..default_networking_config
        };

        let (node, runner) = subspace_networking::construct(config)?;

        let mut destructors = DestructorSet::new_without_async("dsn-destructors");
        let on_new_listener = node.on_new_listener(Arc::new({
            let node = node.clone();

            move |address| {
                tracing::info!(
                    "DSN listening on {}",
                    address
                        .clone()
                        .with(subspace_networking::libp2p::multiaddr::Protocol::P2p(node.id()))
                );
            }
        }));
        destructors.add_items_to_drop(on_new_listener)?;

        Ok((
            DsnShared {
                node,
                farmer_readers_and_pieces,
                _destructors: destructors,
                farmer_piece_cache,
            },
            runner,
            is_metrics_enabled.then_some(metrics_registry),
        ))
    }
}
