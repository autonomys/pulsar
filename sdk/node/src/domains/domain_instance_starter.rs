use std::sync::Arc;

use cross_domain_message_gossip::ChainTxPoolMsg;
use domain_client_operator::OperatorStreams;
use domain_eth_service::provider::EthProvider;
use domain_eth_service::DefaultEthConfig;
use domain_runtime_primitives::opaque::Block as DomainBlock;
use domain_service::{FullBackend, FullClient};
use futures::StreamExt;
use sc_client_api::ImportNotifications;
use sc_consensus_subspace::block_import::BlockImportingNotification;
use sc_consensus_subspace::notification::SubspaceNotificationStream;
use sc_consensus_subspace::slot_worker::NewSlotNotification;
use sc_network::NetworkService;
use sc_service::{Configuration, RpcHandlers};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sc_utils::mpsc::{TracingUnboundedReceiver, TracingUnboundedSender};
use sp_core::H256;
use sp_domains::{DomainId, OperatorId, RuntimeType};
use sp_runtime::traits::NumberFor;
use subspace_runtime::RuntimeApi as CRuntimeApi;
use subspace_runtime_primitives::opaque::Block as CBlock;
use subspace_service::FullClient as CFullClient;
use tokio::task::JoinHandle;

use crate::domains::utils::AccountId20;

/// `DomainInstanceStarter` used to start a domain instance node based on the
/// given bootstrap result
pub struct DomainInstanceStarter {
    pub service_config: Configuration,
    pub maybe_operator_id: Option<OperatorId>,
    pub domain_id: DomainId,
    pub runtime_type: RuntimeType,
    pub additional_arguments: Vec<String>,
    pub consensus_client: Arc<CFullClient<CRuntimeApi>>,
    pub consensus_network: Arc<NetworkService<CBlock, H256>>,
    pub block_importing_notification_stream:
        SubspaceNotificationStream<BlockImportingNotification<CBlock>>,
    pub new_slot_notification_stream: SubspaceNotificationStream<NewSlotNotification>,
    pub consensus_sync_service: Arc<sc_network_sync::SyncingService<CBlock>>,
    pub consensus_offchain_tx_pool_factory: OffchainTransactionPoolFactory<CBlock>,
    pub domain_message_receiver: TracingUnboundedReceiver<ChainTxPoolMsg>,
    pub gossip_message_sink: TracingUnboundedSender<cross_domain_message_gossip::Message>,
}

impl DomainInstanceStarter {
    pub async fn prepare_for_start(
        self,
        domain_created_at: NumberFor<CBlock>,
        imported_block_notification_stream: ImportNotifications<CBlock>,
    ) -> anyhow::Result<(RpcHandlers, JoinHandle<anyhow::Result<()>>)> {
        let DomainInstanceStarter {
            domain_id,
            consensus_network,
            maybe_operator_id,
            runtime_type,
            mut additional_arguments,
            service_config,
            consensus_client,
            block_importing_notification_stream,
            new_slot_notification_stream,
            consensus_sync_service,
            consensus_offchain_tx_pool_factory,
            domain_message_receiver,
            gossip_message_sink,
        } = self;

        let block_importing_notification_stream = || {
            block_importing_notification_stream.subscribe().then(
                |block_importing_notification| async move {
                    (
                        block_importing_notification.block_number,
                        block_importing_notification.acknowledgement_sender,
                    )
                },
            )
        };

        let new_slot_notification_stream = || {
            new_slot_notification_stream.subscribe().then(|slot_notification| async move {
                (
                    slot_notification.new_slot_info.slot,
                    slot_notification.new_slot_info.global_randomness,
                )
            })
        };

        let operator_streams = OperatorStreams {
            // TODO: proper value
            consensus_block_import_throttling_buffer_size: 10,
            block_importing_notification_stream: block_importing_notification_stream(),
            imported_block_notification_stream,
            new_slot_notification_stream: new_slot_notification_stream(),
            _phantom: Default::default(),
            acknowledgement_sender_stream: futures::stream::empty(),
        };

        match runtime_type {
            RuntimeType::Evm => {
                let eth_provider = EthProvider::<
                    evm_domain_runtime::TransactionConverter,
                    DefaultEthConfig<
                        FullClient<DomainBlock, evm_domain_runtime::RuntimeApi>,
                        FullBackend<DomainBlock>,
                    >,
                >::new(
                    Some(service_config.base_path.path()),
                    additional_arguments.drain(..),
                );

                let domain_params = domain_service::DomainParams {
                    domain_id,
                    domain_config: service_config,
                    domain_created_at,
                    maybe_operator_id,
                    consensus_client,
                    consensus_network,
                    consensus_offchain_tx_pool_factory,
                    consensus_network_sync_oracle: consensus_sync_service.clone(),
                    operator_streams,
                    gossip_message_sink,
                    domain_message_receiver,
                    provider: eth_provider,
                    skip_empty_bundle_production: true,
                };

                let mut domain_node = domain_service::new_full::<
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    evm_domain_runtime::RuntimeApi,
                    AccountId20,
                    _,
                    _,
                >(domain_params)
                .await
                .map_err(anyhow::Error::new)?;

                let domain_start_join_handle = sdk_utils::task_spawn(
                    format!("domain-{}/start-domain", <DomainId as Into<u32>>::into(domain_id)),
                    async move {
                        domain_node.network_starter.start_network();
                        domain_node.task_manager.future().await.map_err(anyhow::Error::new)
                    },
                );

                Ok((domain_node.rpc_handlers.clone(), domain_start_join_handle))
            }
        }
    }
}
