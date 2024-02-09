//! This crate is related to abstract farmer implementation

#![warn(
    missing_docs,
    clippy::dbg_macro,
    clippy::unwrap_used,
    clippy::disallowed_types,
    unused_features
)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![feature(const_option)]

use std::collections::HashMap;
use std::io;
use std::num::{NonZeroU8, NonZeroUsize};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context};
pub use builder::{Builder, Config};
use derivative::Derivative;
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use sdk_traits::Node;
use sdk_utils::{ByteSize, DestructorSet, PublicKey, TaskOutput};
use serde::{Deserialize, Serialize};
use subspace_core_primitives::crypto::kzg;
use subspace_core_primitives::{PieceIndex, Record, SectorIndex};
use subspace_erasure_coding::ErasureCoding;
use subspace_farmer::piece_cache::PieceCache as FarmerPieceCache;
use subspace_farmer::single_disk_farm::{
    SectorPlottingDetails, SectorUpdate, SingleDiskFarm, SingleDiskFarmError, SingleDiskFarmId,
    SingleDiskFarmInfo, SingleDiskFarmOptions, SingleDiskFarmSummary,
};
use subspace_farmer::thread_pool_manager::PlottingThreadPoolManager;
use subspace_farmer::utils::farmer_piece_getter::FarmerPieceGetter;
use subspace_farmer::utils::piece_validator::SegmentCommitmentPieceValidator;
use subspace_farmer::utils::readers_and_pieces::ReadersAndPieces;
use subspace_farmer::utils::{
    all_cpu_cores, create_plotting_thread_pool_manager, thread_pool_core_indices,
};
use subspace_farmer::{Identity, KNOWN_PEERS_CACHE_SIZE};
use subspace_farmer_components::plotting::PlottedSector;
use subspace_farmer_components::sector::{sector_size, SectorMetadataChecksummed};
use subspace_networking::libp2p::kad::RecordKey;
use subspace_networking::utils::multihash::ToMultihash;
use subspace_networking::KnownPeersManager;
use subspace_rpc_primitives::{FarmerAppInfo, SolutionResponse};
use tokio::sync::{mpsc, oneshot, watch, Mutex, Semaphore};
use tracing::{debug, error, info, warn};
use tracing_futures::Instrument;

/// Description of the farm
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub struct FarmDescription {
    /// Path of the farm
    pub directory: PathBuf,
    /// Space which you want to pledge
    pub space_pledged: ByteSize,
}

impl FarmDescription {
    /// Construct Farm description
    pub fn new(directory: impl Into<PathBuf>, space_pledged: ByteSize) -> Self {
        Self { directory: directory.into(), space_pledged }
    }

    /// Wipe all the data from the farm
    pub async fn wipe(self) -> io::Result<()> {
        tokio::fs::remove_dir_all(self.directory).await
    }
}

mod builder {
    use std::num::{NonZeroU8, NonZeroUsize};

    use derivative::Derivative;
    use derive_builder::Builder;
    use derive_more::{Deref, DerefMut, Display, From};
    use sdk_traits::Node;
    use sdk_utils::{ByteSize, PublicKey};
    use serde::{Deserialize, Serialize};

    use super::BuildError;
    use crate::{FarmDescription, Farmer};

    #[derive(
        Debug,
        Clone,
        Derivative,
        Deserialize,
        Serialize,
        PartialEq,
        Eq,
        From,
        Deref,
        DerefMut,
        Display,
    )]
    #[derivative(Default)]
    #[serde(transparent)]
    pub struct MaxConcurrentFarms(
        #[derivative(Default(value = "NonZeroUsize::new(10).expect(\"10 > 0\")"))]
        pub(crate)  NonZeroUsize,
    );

    #[derive(
        Debug,
        Clone,
        Derivative,
        Deserialize,
        Serialize,
        PartialEq,
        Eq,
        From,
        Deref,
        DerefMut,
        Display,
    )]
    #[derivative(Default)]
    #[serde(transparent)]
    pub struct PieceCacheSize(
        #[derivative(Default(value = "ByteSize::mib(10)"))] pub(crate) ByteSize,
    );

    #[derive(
        Debug,
        Clone,
        Derivative,
        Deserialize,
        Serialize,
        PartialEq,
        Eq,
        From,
        Deref,
        DerefMut,
        Display,
    )]
    #[derivative(Default)]
    #[serde(transparent)]
    pub struct ProvidedKeysLimit(
        #[derivative(Default(value = "NonZeroUsize::new(655360).expect(\"655360 > 0\")"))]
        pub(crate) NonZeroUsize,
    );

    /// Technical type which stores all
    #[derive(Debug, Clone, Derivative, Builder, Serialize, Deserialize)]
    #[derivative(Default)]
    #[builder(pattern = "immutable", build_fn(private, name = "_build"), name = "Builder")]
    #[non_exhaustive]
    pub struct Config {
        /// Number of farms that can be plotted concurrently, impacts RAM usage.
        #[builder(default, setter(into))]
        #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
        pub max_concurrent_farms: MaxConcurrentFarms,
        /// Number of farms that can be farmted concurrently, impacts RAM usage.
        #[builder(default, setter(into))]
        #[serde(default, skip_serializing_if = "sdk_utils::is_default")]
        pub provided_keys_limit: ProvidedKeysLimit,
        /// Maximum number of pieces in single sector
        #[builder(default)]
        pub max_pieces_in_sector: Option<u16>,
        /// Size of PER FARM thread pool used for farming (mostly for blocking
        /// I/O, but also for some compute-intensive operations during
        /// proving), defaults to number of logical CPUs
        /// available on UMA system and number of logical CPUs in
        /// first NUMA node on NUMA system.
        #[builder(default)]
        pub farming_thread_pool_size: Option<NonZeroUsize>,
        /// Size of one thread pool used for plotting, defaults to number of
        /// logical CPUs available on UMA system and number of logical
        /// CPUs available in NUMA node on NUMA system.
        ///
        /// Number of thread pools is defined by `--sector-encoding-concurrency`
        /// option, different thread pools might have different number
        /// of threads if NUMA nodes do not have the same size.
        ///
        /// Threads will be pinned to corresponding CPU cores at creation.
        #[builder(default)]
        pub plotting_thread_pool_size: Option<NonZeroUsize>,
        /// the plotting process, defaults to `--sector-downloading-concurrency`
        /// + 1 to download future sector ahead of time
        #[builder(default)]
        pub sector_downloading_concurrency: Option<NonZeroUsize>,
        /// Defines how many sectors farmer will encode concurrently, defaults
        /// to 1 on UMA system and number of NUMA nodes on NUMA system.
        /// It is further restricted by `sector_downloading_concurrency`
        /// and setting this option higher than
        /// `sector_downloading_concurrency` will have no effect.
        #[builder(default)]
        pub sector_encoding_concurrency: Option<NonZeroUsize>,
        /// Threads will be pinned to corresponding CPU cores at creation.
        #[builder(default)]
        pub replotting_thread_pool_size: Option<NonZeroUsize>,
    }

    impl Builder {
        /// Get configuration for saving on disk
        pub fn configuration(&self) -> Config {
            self._build().expect("Build is infallible")
        }

        /// Open and start farmer
        pub async fn build<N: Node>(
            self,
            reward_address: PublicKey,
            node: &N,
            farms: &[FarmDescription],
            cache_percentage: NonZeroU8,
        ) -> Result<Farmer<N::Table>, BuildError> {
            self.configuration().build(reward_address, node, farms, cache_percentage).await
        }
    }
}

/// Error when farm creation fails
#[derive(Debug, thiserror::Error)]
pub enum SingleDiskFarmCreationError {
    /// Insufficient disk while creating single disk farm
    #[error("Unable to create farm as Allocated space {} ({}) is not enough, minimum is ~{} (~{}, {} bytes to be exact", bytesize::to_string(*.allocated_space, true), bytesize::to_string(*.allocated_space, false), bytesize::to_string(*.min_space, true), bytesize::to_string(*.min_space, false), *.min_space)]
    InsufficientSpaceForFarm {
        /// Minimum space required for farm
        min_space: u64,
        /// Allocated space for farm
        allocated_space: u64,
    },
    /// Other error while creating single disk farm
    #[error("Single disk farm creation error: {0}")]
    Other(#[from] SingleDiskFarmError),
}

/// Build Error
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// Failed to create single disk farm
    #[error("Single disk farm creation error: {0}")]
    SingleDiskFarmCreate(#[from] SingleDiskFarmCreationError),
    /// No farms were supplied during building
    #[error("Supply at least one farm")]
    NoFarmsSupplied,
    /// Failed to fetch data from the node
    #[error("Failed to fetch data from node: {0}")]
    RPCError(#[source] subspace_farmer::RpcClientError),
    /// Failed to build thread pool
    #[error("Failed to build thread pool: {0}")]
    ThreadPoolError(#[from] rayon::ThreadPoolBuildError),
    /// Other error
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

#[async_trait::async_trait]
impl<T: subspace_proof_of_space::Table> sdk_traits::Farmer for Farmer<T> {
    type Table = T;

    async fn get_piece_by_index(
        piece_index: PieceIndex,
        piece_cache: &FarmerPieceCache,
        weak_readers_and_pieces: &std::sync::Weak<parking_lot::Mutex<Option<ReadersAndPieces>>>,
    ) -> Option<subspace_core_primitives::Piece> {
        use tracing::debug;

        if let Some(piece) =
            piece_cache.get_piece(RecordKey::from(piece_index.to_multihash())).await
        {
            return Some(piece);
        }

        let weak_readers_and_pieces = weak_readers_and_pieces.clone();

        debug!(?piece_index, "No piece in the cache. Trying archival storage...");

        let readers_and_pieces = match weak_readers_and_pieces.upgrade() {
            Some(readers_and_pieces) => readers_and_pieces,
            None => {
                debug!("A readers and pieces are already dropped");
                return None;
            }
        };
        let read_piece = match readers_and_pieces.lock().as_ref() {
            Some(readers_and_pieces) => readers_and_pieces.read_piece(&piece_index),
            None => {
                debug!(?piece_index, "Readers and pieces are not initialized yet");
                return None;
            }
        };

        match read_piece {
            Some(fut) => fut.in_current_span().await,
            None => None,
        }
    }
}

const SEGMENT_COMMITMENTS_CACHE_SIZE: NonZeroUsize =
    NonZeroUsize::new(1_000_000).expect("Not zero; qed");

async fn create_readers_and_pieces(
    single_disk_farms: &[SingleDiskFarm],
) -> Result<ReadersAndPieces, BuildError> {
    // Store piece readers so we can reference them later
    let readers = single_disk_farms.iter().map(SingleDiskFarm::piece_reader).collect();
    let mut readers_and_pieces = ReadersAndPieces::new(readers);

    tracing::debug!("Collecting already plotted pieces");

    let mut plotted_sectors_iters = futures::future::join_all(
        single_disk_farms.iter().map(|single_disk_farm| single_disk_farm.plotted_sectors()),
    )
    .await;

    plotted_sectors_iters.drain(..).enumerate().try_for_each(
        |(disk_farm_index, plotted_sectors_iter)| {
            let disk_farm_index = disk_farm_index.try_into().map_err(|_error| {
                anyhow!(
                    "More than 256 farms are not supported, consider running multiple farmer \
                     instances"
                )
            })?;

            (0 as SectorIndex..).zip(plotted_sectors_iter).for_each(
                |(sector_index, plotted_sector_result)| match plotted_sector_result {
                    Ok(plotted_sector) => {
                        readers_and_pieces.add_sector(disk_farm_index, &plotted_sector);
                    }
                    Err(error) => {
                        error!(
                            %error,
                            %disk_farm_index,
                            %sector_index,
                            "Failed reading plotted sector on startup, skipping"
                        );
                    }
                },
            );

            Ok::<_, anyhow::Error>(())
        },
    )?;

    tracing::debug!("Finished collecting already plotted pieces");

    Ok(readers_and_pieces)
}

fn handler_on_sector_plotted(
    plotted_sector: &PlottedSector,
    maybe_old_plotted_sector: &Option<PlottedSector>,
    disk_farm_index: usize,
    readers_and_pieces: Arc<parking_lot::Mutex<Option<ReadersAndPieces>>>,
) {
    let disk_farm_index = disk_farm_index
        .try_into()
        .expect("More than 256 farms are not supported, this is checked above already; qed");

    {
        let mut readers_and_pieces = readers_and_pieces.lock();
        let readers_and_pieces =
            readers_and_pieces.as_mut().expect("Initial value was populated before; qed");

        if let Some(old_plotted_sector) = maybe_old_plotted_sector {
            readers_and_pieces.delete_sector(disk_farm_index, old_plotted_sector);
        }
        readers_and_pieces.add_sector(disk_farm_index, plotted_sector);
    }
}

impl Config {
    /// Open and start farmer
    pub async fn build<N: Node, T: subspace_proof_of_space::Table>(
        self,
        reward_address: PublicKey,
        node: &N,
        farms: &[FarmDescription],
        cache_percentage: NonZeroU8,
    ) -> Result<Farmer<T>, BuildError> {
        if farms.is_empty() {
            return Err(BuildError::NoFarmsSupplied);
        }

        let mut destructors = DestructorSet::new("farmer-destructors");

        let Self {
            max_concurrent_farms: _,
            provided_keys_limit: _,
            max_pieces_in_sector,
            farming_thread_pool_size,
            plotting_thread_pool_size,
            replotting_thread_pool_size,
            sector_downloading_concurrency,
            sector_encoding_concurrency,
        } = self;

        let mut single_disk_farms = Vec::with_capacity(farms.len());
        let mut farm_info = HashMap::with_capacity(farms.len());

        let readers_and_pieces = Arc::clone(&node.dsn().farmer_readers_and_pieces);

        let node_name = node.name().to_owned();

        let peer_id = node.dsn().node.id();

        let (farmer_piece_cache, farmer_piece_cache_worker) =
            FarmerPieceCache::new(node.rpc().clone(), peer_id);

        let kzg = kzg::Kzg::new(kzg::embedded_kzg_settings());
        let erasure_coding = ErasureCoding::new(
            NonZeroUsize::new(Record::NUM_S_BUCKETS.next_power_of_two().ilog2() as usize).expect(
                "Number of buckets >= 1, therefore next power of 2 >= 2, therefore ilog2 >= 1",
            ),
        )
        .map_err(|error| anyhow::anyhow!("Failed to create erasure coding for farm: {error}"))?;

        let piece_provider = subspace_networking::utils::piece_provider::PieceProvider::new(
            node.dsn().node.clone(),
            Some(SegmentCommitmentPieceValidator::new(
                node.dsn().node.clone(),
                node.rpc().clone(),
                kzg.clone(),
                // TODO: Consider introducing and using global in-memory segment commitments cache
                Arc::new(parking_lot::Mutex::new(lru::LruCache::new(
                    SEGMENT_COMMITMENTS_CACHE_SIZE,
                ))),
            )),
        );
        let farmer_piece_getter = Arc::new(FarmerPieceGetter::new(
            piece_provider,
            farmer_piece_cache.clone(),
            node.rpc().clone(),
            readers_and_pieces.clone(),
        ));

        let (piece_cache_worker_drop_sender, piece_cache_worker_drop_receiver) =
            oneshot::channel::<()>();
        let farmer_piece_cache_worker_join_handle = sdk_utils::task_spawn_blocking(
            format!("sdk-farmer-{node_name}-pieces-cache-worker"),
            {
                let handle = tokio::runtime::Handle::current();
                let piece_getter = farmer_piece_getter.clone();

                move || {
                    handle.block_on(future::select(
                        Box::pin({
                            let piece_getter = piece_getter.clone();
                            farmer_piece_cache_worker.run(piece_getter)
                        }),
                        piece_cache_worker_drop_receiver,
                    ));
                }
            },
        );

        destructors.add_async_destructor({
            async move {
                let _ = piece_cache_worker_drop_sender.send(());
                farmer_piece_cache_worker_join_handle.await.expect(
                    "awaiting worker should not fail except panic by the worker itself; qed",
                );
            }
        })?;

        let farmer_app_info = subspace_farmer::NodeClient::farmer_app_info(node.rpc())
            .await
            .expect("Node is always reachable");

        let max_pieces_in_sector = match max_pieces_in_sector {
            Some(m) => m,
            None => farmer_app_info.protocol_info.max_pieces_in_sector,
        };

        let mut plotting_delay_senders = Vec::with_capacity(farms.len());

        let plotting_thread_pool_core_indices =
            thread_pool_core_indices(plotting_thread_pool_size, sector_encoding_concurrency);
        let replotting_thread_pool_core_indices = {
            let mut replotting_thread_pool_core_indices =
                thread_pool_core_indices(replotting_thread_pool_size, sector_encoding_concurrency);
            if replotting_thread_pool_size.is_none() {
                // The default behavior is to use all CPU cores, but for replotting we just want
                // half
                replotting_thread_pool_core_indices
                    .iter_mut()
                    .for_each(|set| set.truncate(set.cpu_cores().len() / 2));
            }
            replotting_thread_pool_core_indices
        };

        let downloading_semaphore = Arc::new(Semaphore::new(
            sector_downloading_concurrency
                .map(|sector_downloading_concurrency| sector_downloading_concurrency.get())
                .unwrap_or(plotting_thread_pool_core_indices.len() + 1),
        ));

        let all_cpu_cores = all_cpu_cores();
        let plotting_thread_pool_manager = create_plotting_thread_pool_manager(
            plotting_thread_pool_core_indices.into_iter().zip(replotting_thread_pool_core_indices),
        )?;
        let farming_thread_pool_size = farming_thread_pool_size
            .map(|farming_thread_pool_size| farming_thread_pool_size.get())
            .unwrap_or_else(|| {
                all_cpu_cores
                    .first()
                    .expect("Not empty according to function description; qed")
                    .cpu_cores()
                    .len()
            });

        if all_cpu_cores.len() > 1 {
            info!(numa_nodes = %all_cpu_cores.len(), "NUMA system detected");

            if all_cpu_cores.len() > farms.len() {
                warn!(
                    numa_nodes = %all_cpu_cores.len(),
                    farms_count = %farms.len(),
                    "Too few disk farms, CPU will not be utilized fully during plotting, same number of farms as NUMA \
                    nodes or more is recommended"
                );
            }
        }

        // TODO: Remove code or environment variable once identified whether it helps or
        // not
        if std::env::var("NUMA_ALLOCATOR").is_ok() && all_cpu_cores.len() > 1 {
            unsafe {
                libmimalloc_sys::mi_option_set(
                    libmimalloc_sys::mi_option_use_numa_nodes,
                    all_cpu_cores.len() as std::ffi::c_long,
                );
            }
        }

        for (disk_farm_idx, description) in farms.iter().enumerate() {
            let (plotting_delay_sender, plotting_delay_receiver) =
                futures::channel::oneshot::channel();
            plotting_delay_senders.push(plotting_delay_sender);

            let (farm, single_disk_farm) = Farm::new(FarmOptions {
                disk_farm_idx,
                cache_percentage,
                reward_address,
                node,
                max_pieces_in_sector,
                piece_getter: Arc::clone(&farmer_piece_getter),
                description,
                kzg: kzg.clone(),
                erasure_coding: erasure_coding.clone(),
                farming_thread_pool_size,
                plotting_delay: Some(plotting_delay_receiver),
                downloading_semaphore: Arc::clone(&downloading_semaphore),
                plotting_thread_pool_manager: plotting_thread_pool_manager.clone(),
            })
            .await?;

            farm_info.insert(farm.directory.clone(), farm);
            single_disk_farms.push(single_disk_farm);
        }

        *node.dsn().farmer_piece_cache.write() = Some(farmer_piece_cache.clone());
        destructors.add_sync_destructor({
            let piece_cache = Arc::clone(&node.dsn().farmer_piece_cache);
            move || {
                piece_cache.write().take();
            }
        })?;

        let cache_acknowledgement_receiver = farmer_piece_cache
            .replace_backing_caches(
                single_disk_farms
                    .iter()
                    .map(|single_disk_farm| single_disk_farm.piece_cache())
                    .collect(),
            )
            .await;
        drop(farmer_piece_cache);

        let (plotting_delay_task_drop_sender, plotting_delay_task_drop_receiver) =
            oneshot::channel::<()>();
        let plotting_delay_task_join_handle = sdk_utils::task_spawn_blocking(
            format!("sdk-farmer-{node_name}-plotting-delay-task"),
            {
                let handle = tokio::runtime::Handle::current();

                move || {
                    handle.block_on(future::select(
                        Box::pin(async {
                            if cache_acknowledgement_receiver.await.is_ok() {
                                for plotting_delay_sender in plotting_delay_senders {
                                    // Doesn't matter if receiver is gone
                                    let _ = plotting_delay_sender.send(());
                                }
                            }
                        }),
                        plotting_delay_task_drop_receiver,
                    ));
                }
            },
        );

        destructors.add_async_destructor({
            async move {
                let _ = plotting_delay_task_drop_sender.send(());
                plotting_delay_task_join_handle.await.expect(
                    "awaiting worker should not fail except panic by the worker itself; qed",
                );
            }
        })?;

        let readers_and_pieces_instance = create_readers_and_pieces(&single_disk_farms).await?;
        readers_and_pieces.lock().replace(readers_and_pieces_instance);
        destructors.add_sync_destructor({
            let farmer_reader_and_pieces = node.dsn().farmer_readers_and_pieces.clone();
            move || {
                farmer_reader_and_pieces.lock().take();
            }
        })?;

        let mut sector_plotting_handler_ids = vec![];
        for (disk_farm_index, single_disk_farm) in single_disk_farms.iter().enumerate() {
            let readers_and_pieces = Arc::clone(&readers_and_pieces);
            let span = tracing::info_span!("farm", %disk_farm_index);

            // Collect newly plotted pieces
            // TODO: Once we have replotting, this will have to be updated
            sector_plotting_handler_ids.push(single_disk_farm.on_sector_update(Arc::new(
                move |(_plotted_sector, sector_update)| {
                    let _span_guard = span.enter();

                    let (plotted_sector, maybe_old_plotted_sector) = match sector_update {
                        SectorUpdate::Plotting(SectorPlottingDetails::Finished {
                            plotted_sector,
                            old_plotted_sector,
                            ..
                        }) => (plotted_sector, old_plotted_sector),
                        _ => return,
                    };

                    handler_on_sector_plotted(
                        plotted_sector,
                        maybe_old_plotted_sector,
                        disk_farm_index,
                        readers_and_pieces.clone(),
                    )
                },
            )));
        }

        let mut single_disk_farms_stream =
            single_disk_farms.into_iter().map(SingleDiskFarm::run).collect::<FuturesUnordered<_>>();

        let (farm_driver_drop_sender, mut farm_driver_drop_receiver) = oneshot::channel::<()>();
        let (farm_driver_result_sender, farm_driver_result_receiver) =
            mpsc::channel::<_>(u8::MAX as usize + 1);

        let farm_driver_join_handle =
            sdk_utils::task_spawn_blocking(format!("sdk-farmer-{node_name}-farms-driver"), {
                let handle = tokio::runtime::Handle::current();

                move || {
                    use future::Either::*;

                    loop {
                        let result = handle.block_on(future::select(
                            single_disk_farms_stream.next(),
                            &mut farm_driver_drop_receiver,
                        ));

                        match result {
                            Left((maybe_result, _)) => {
                                let send_result = match maybe_result {
                                    None => farm_driver_result_sender
                                        .try_send(Ok(TaskOutput::Value(None))),
                                    Some(result) => match result {
                                        Ok(single_disk_farm_id) => farm_driver_result_sender
                                            .try_send(Ok(TaskOutput::Value(Some(
                                                single_disk_farm_id,
                                            )))),
                                        Err(e) => farm_driver_result_sender.try_send(Err(e)),
                                    },
                                };

                                // Receiver is closed which would mean we are shutting down
                                if send_result.is_err() {
                                    break;
                                }
                            }
                            Right((_, _)) => {
                                warn!("Received drop signal for farm driver, exiting...");
                                let _ =
                                    farm_driver_result_sender.try_send(Ok(TaskOutput::Cancelled(
                                        "Received drop signal for farm driver".into(),
                                    )));
                                break;
                            }
                        };
                    }
                }
            });

        destructors.add_async_destructor({
            async move {
                let _ = farm_driver_drop_sender.send(());
                farm_driver_join_handle.await.expect("joining should not fail; qed");
            }
        })?;

        for handler_id in sector_plotting_handler_ids.drain(..) {
            destructors.add_items_to_drop(handler_id)?;
        }

        tracing::debug!("Started farmer");

        Ok(Farmer {
            reward_address,
            farm_info,
            result_receiver: Some(farm_driver_result_receiver),
            node_name,
            app_info: subspace_farmer::NodeClient::farmer_app_info(node.rpc())
                .await
                .expect("Node is always reachable"),
            _destructors: destructors,
        })
    }
}

type ResultReceiver = mpsc::Receiver<anyhow::Result<TaskOutput<Option<SingleDiskFarmId>, String>>>;

/// Farmer structure
#[derive(Derivative)]
#[derivative(Debug)]
#[must_use = "Farmer should be closed"]
pub struct Farmer<T: subspace_proof_of_space::Table> {
    reward_address: PublicKey,
    farm_info: HashMap<PathBuf, Farm<T>>,
    result_receiver: Option<ResultReceiver>,
    node_name: String,
    app_info: FarmerAppInfo,
    _destructors: DestructorSet,
}

/// Info about some farm
#[derive(Debug)]
#[non_exhaustive]
// TODO: Should it be versioned?
pub struct FarmInfo {
    /// ID of the farm
    pub id: SingleDiskFarmId,
    /// Genesis hash of the chain used for farm creation
    pub genesis_hash: [u8; 32],
    /// Public key of identity used for farm creation
    pub public_key: PublicKey,
    /// How much space in bytes is allocated for this farm
    pub allocated_space: ByteSize,
    /// How many pieces are in sector
    pub pieces_in_sector: u16,
}

impl From<SingleDiskFarmInfo> for FarmInfo {
    fn from(info: SingleDiskFarmInfo) -> Self {
        let SingleDiskFarmInfo::V0 {
            id,
            genesis_hash,
            public_key,
            allocated_space,
            pieces_in_sector,
        } = info;
        Self {
            id,
            genesis_hash,
            public_key: PublicKey(public_key),
            allocated_space: ByteSize::b(allocated_space),
            pieces_in_sector,
        }
    }
}

/// Farmer info
#[derive(Debug)]
#[non_exhaustive]
pub struct Info {
    /// Version of the farmer
    pub version: String,
    /// Reward address of our farmer
    pub reward_address: PublicKey,
    // TODO: add dsn peers info
    // pub dsn_peers: u64,
    /// Info about each farm
    pub farms_info: HashMap<PathBuf, FarmInfo>,
    /// Sector size in bits
    pub sector_size: u64,
}

/// Initial plotting progress
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitialPlottingProgress {
    /// Number of sectors from which we started plotting
    pub starting_sector: u64,
    /// Current number of sectors
    pub current_sector: u64,
    /// Total number of sectors on disk
    pub total_sectors: u64,
}

/// Progress data received from sender, used to monitor plotting progress
pub type ProgressData = Option<(u16, SectorUpdate)>;

/// Farm structure
#[derive(Debug)]
pub struct Farm<T: subspace_proof_of_space::Table> {
    directory: PathBuf,
    progress: watch::Receiver<ProgressData>,
    solutions: watch::Receiver<Option<SolutionResponse>>,
    initial_plotting_progress: Arc<Mutex<InitialPlottingProgress>>,
    allocated_space: u64,
    _destructors: DestructorSet,
    _table: std::marker::PhantomData<T>,
}

#[pin_project::pin_project]
struct InitialPlottingProgressStreamInner<S> {
    last_initial_plotting_progress: InitialPlottingProgress,
    #[pin]
    stream: S,
}

impl<S: Stream> Stream for InitialPlottingProgressStreamInner<S>
where
    S: Stream<Item = InitialPlottingProgress>,
{
    type Item = InitialPlottingProgress;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.project();
        match this.stream.poll_next(cx) {
            result @ std::task::Poll::Ready(Some(progress)) => {
                *this.last_initial_plotting_progress = progress;
                result
            }
            result => result,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.last_initial_plotting_progress.total_sectors
            - self.last_initial_plotting_progress.current_sector;
        (left as usize, Some(left as usize))
    }
}

/// Initial plotting progress stream
#[pin_project::pin_project]
pub struct InitialPlottingProgressStream {
    #[pin]
    boxed_stream:
        std::pin::Pin<Box<dyn Stream<Item = InitialPlottingProgress> + Send + Sync + Unpin>>,
}

impl Stream for InitialPlottingProgressStream {
    type Item = InitialPlottingProgress;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.project().boxed_stream.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.boxed_stream.size_hint()
    }
}

struct FarmOptions<'a, PG, N: sdk_traits::Node> {
    pub disk_farm_idx: usize,
    pub cache_percentage: NonZeroU8,
    pub reward_address: PublicKey,
    pub node: &'a N,
    pub piece_getter: PG,
    pub description: &'a FarmDescription,
    pub kzg: kzg::Kzg,
    pub erasure_coding: ErasureCoding,
    pub max_pieces_in_sector: u16,
    pub farming_thread_pool_size: usize,
    pub plotting_delay: Option<futures::channel::oneshot::Receiver<()>>,
    pub downloading_semaphore: Arc<Semaphore>,
    pub plotting_thread_pool_manager: PlottingThreadPoolManager,
}

impl<T: subspace_proof_of_space::Table> Farm<T> {
    async fn new(
        FarmOptions {
            disk_farm_idx,
            cache_percentage,
            reward_address,
            node,
            piece_getter,
            description,
            kzg,
            erasure_coding,
            max_pieces_in_sector,
            farming_thread_pool_size,
            plotting_delay,
            downloading_semaphore,
            plotting_thread_pool_manager,
        }: FarmOptions<
            '_,
            impl subspace_farmer_components::plotting::PieceGetter + Clone + Send + Sync + 'static,
            impl sdk_traits::Node,
        >,
    ) -> Result<(Self, SingleDiskFarm), BuildError> {
        let directory = description.directory.clone();
        let allocated_space = description.space_pledged.as_u64();
        let farmer_app_info = subspace_farmer::NodeClient::farmer_app_info(node.rpc())
            .await
            .expect("Node is always reachable");

        let description = SingleDiskFarmOptions {
            allocated_space,
            directory: directory.clone(),
            farmer_app_info,
            max_pieces_in_sector,
            reward_address: *reward_address,
            node_client: node.rpc().clone(),
            kzg,
            erasure_coding,
            piece_getter,
            cache_percentage,
            downloading_semaphore,
            farm_during_initial_plotting: false,
            farming_thread_pool_size,
            plotting_thread_pool_manager,
            plotting_delay,
        };
        let single_disk_farm_fut = SingleDiskFarm::new::<_, _, T>(description, disk_farm_idx);
        let single_disk_farm = match single_disk_farm_fut.await {
            Ok(single_disk_farm) => single_disk_farm,
            Err(SingleDiskFarmError::InsufficientAllocatedSpace { min_space, allocated_space }) => {
                return Err(BuildError::SingleDiskFarmCreate(
                    SingleDiskFarmCreationError::InsufficientSpaceForFarm {
                        min_space,
                        allocated_space,
                    },
                ));
            }
            Err(error) => {
                return Err(BuildError::SingleDiskFarmCreate(SingleDiskFarmCreationError::Other(
                    error,
                )));
            }
        };
        let mut destructors = DestructorSet::new_without_async("farm-destructors");

        let progress = {
            let (sender, receiver) = watch::channel::<Option<_>>(None);
            destructors.add_items_to_drop(single_disk_farm.on_sector_update(Arc::new(
                move |sector| {
                    let _ = sender.send(Some(sector.clone()));
                },
            )))?;
            receiver
        };
        let solutions = {
            let (sender, receiver) = watch::channel::<Option<_>>(None);
            destructors.add_items_to_drop(single_disk_farm.on_solution(Arc::new(
                move |solution| {
                    let _ = sender.send(Some(solution.clone()));
                },
            )))?;
            receiver
        };

        // TODO: This calculation is directly imported from the monorepo and relies on
        // internal calculation of farm. Remove it once we have public function.
        let fixed_space_usage = 2 * 1024 * 1024
            + Identity::file_size() as u64
            + KnownPeersManager::file_size(KNOWN_PEERS_CACHE_SIZE) as u64;
        // Calculate how many sectors can fit
        let target_sector_count = {
            let potentially_plottable_space = allocated_space.saturating_sub(fixed_space_usage)
                / 100
                * (100 - u64::from(cache_percentage.get()));
            // Do the rounding to make sure we have exactly as much space as fits whole
            // number of sectors
            potentially_plottable_space
                / (sector_size(max_pieces_in_sector) + SectorMetadataChecksummed::encoded_size())
                    as u64
        };

        Ok((
            Self {
                directory: directory.clone(),
                allocated_space,
                progress,
                solutions,
                initial_plotting_progress: Arc::new(Mutex::new(InitialPlottingProgress {
                    starting_sector: u64::try_from(single_disk_farm.plotted_sectors_count().await)
                        .expect("Sector count is less than u64::MAX"),
                    current_sector: u64::try_from(single_disk_farm.plotted_sectors_count().await)
                        .expect("Sector count is less than u64::MAX"),
                    total_sectors: target_sector_count,
                })),
                _destructors: destructors,
                _table: Default::default(),
            },
            single_disk_farm,
        ))
    }

    /// Farm location
    pub fn directory(&self) -> &PathBuf {
        &self.directory
    }

    /// Farm size
    pub fn allocated_space(&self) -> ByteSize {
        ByteSize::b(self.allocated_space)
    }

    /// Will return a stream of initial plotting progress which will end once we
    /// finish plotting
    pub async fn subscribe_initial_plotting_progress(&self) -> InitialPlottingProgressStream {
        let initial = *self.initial_plotting_progress.lock().await;
        if initial.current_sector == initial.total_sectors {
            return InitialPlottingProgressStream {
                boxed_stream: Box::pin(futures::stream::iter(None)),
            };
        }

        let stream = tokio_stream::wrappers::WatchStream::new(self.progress.clone())
            .filter_map({
                let initial_plotting_progress = Arc::clone(&self.initial_plotting_progress);
                move |_| {
                    let initial_plotting_progress = Arc::clone(&initial_plotting_progress);
                    async move {
                        let mut guard = initial_plotting_progress.lock().await;
                        let plotting_progress = *guard;
                        guard.current_sector += 1;
                        Some(plotting_progress)
                    }
                }
            })
            .take_while(|InitialPlottingProgress { current_sector, total_sectors, .. }| {
                futures::future::ready(current_sector < total_sectors)
            });
        let last_initial_plotting_progress = *self.initial_plotting_progress.lock().await;

        InitialPlottingProgressStream {
            boxed_stream: Box::pin(Box::pin(InitialPlottingProgressStreamInner {
                stream,
                last_initial_plotting_progress,
            })),
        }
    }

    /// New solution subscription
    pub async fn subscribe_new_solutions(
        &self,
    ) -> impl Stream<Item = SolutionResponse> + Send + Sync + Unpin {
        tokio_stream::wrappers::WatchStream::new(self.solutions.clone())
            .filter_map(futures::future::ready)
    }
}

impl<T: subspace_proof_of_space::Table> Farmer<T> {
    /// Farmer builder
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Gets farm info
    pub async fn get_info(&self) -> anyhow::Result<Info> {
        let farms_info = tokio::task::spawn_blocking({
            let dirs = self.farm_info.keys().cloned().collect::<Vec<_>>();
            || dirs.into_iter().map(SingleDiskFarm::collect_summary).collect::<Vec<_>>()
        })
        .await?
        .into_iter()
        .map(|summary| match summary {
            SingleDiskFarmSummary::Found { info, directory } => Ok((directory, info.into())),
            SingleDiskFarmSummary::NotFound { directory } =>
                Err(anyhow::anyhow!("Didn't found farm at `{directory:?}'")),
            SingleDiskFarmSummary::Error { directory, error } =>
                Err(error).context(format!("Failed to get farm summary at `{directory:?}'")),
        })
        .collect::<anyhow::Result<_>>()?;

        Ok(Info {
            farms_info,
            version: format!("{}-{}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH")),
            reward_address: self.reward_address,
            sector_size: subspace_farmer_components::sector::sector_size(
                self.app_info.protocol_info.max_pieces_in_sector,
            ) as _,
        })
    }

    /// Iterate over farms
    pub async fn iter_farms(&'_ self) -> impl Iterator<Item = &'_ Farm<T>> + '_ {
        self.farm_info.values()
    }

    /// Stops farming, closes farms, and sends signal to the node
    pub async fn close(mut self) -> anyhow::Result<()> {
        self._destructors.async_drop().await?;
        let mut result_receiver = self.result_receiver.take().expect("Handle is always there");
        result_receiver.close();
        while let Some(task_result) = result_receiver.recv().await {
            let output = task_result?;
            match output {
                TaskOutput::Value(_) => {}
                TaskOutput::Cancelled(reason) => {
                    warn!("Farm driver was cancelled due to reason: {:?}", reason);
                }
            }
        }

        Ok(())
    }
}
