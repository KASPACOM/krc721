use crate::imports::*;
use kaspa_rpc_core::api::ctl::{RpcCtl, RpcState};
use kaspa_rpc_core::{
    api::ops::{RPC_API_REVISION, RPC_API_VERSION},
    model::GetServerInfoResponse,
    notify::connection::{ChannelConnection, ChannelType},
    Notification, VirtualChainChangedNotification, VirtualDaaScoreChangedNotification,
};
use kaspa_wallet_core::rpc::{DynRpcApi, Rpc};

use crate::notifier::Notifier;
use crate::state::State;
use crate::syncer::SyncerT;
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, VirtualChainChangedScope, VirtualDaaScoreChangedScope},
};
use kaspa_wrpc_client::prelude::{ConnectOptions, KaspaRpcClient};
use krc721_core::model::krc721::{BlueScoredChainBlockHash, DataT};
use tracing::{error, info, info_span, Instrument};

pub enum NexusMode {
    Indexer { processor: Arc<Processor> },
    Capture { database_name: String },
}

struct Inner {
    network_id: NetworkId,
    rpc: Mutex<Rpc>,
    notification_channel: Channel<Notification>,
    listener_id: RwLock<Option<ListenerId>>,
    consumer: Arc<dyn ConsumerT>,
    syncer: Option<Arc<dyn SyncerT>>,
    accessor: Arc<Accessor>,
    shutdown: DuplexChannel<()>,
    state: Arc<State>,
    counters: Arc<Counters>,
    trace_sync: bool,
    #[allow(unused)]
    notifier: Option<Notifier>,
}

#[derive(Clone)]
pub struct Nexus {
    #[allow(dead_code)]
    inner: Arc<Inner>,
}

impl Nexus {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rpc: Rpc,
        state: Arc<State>,
        counters: Arc<Counters>,
        consumer: Arc<dyn ConsumerT>,
        syncer: Option<Arc<dyn SyncerT>>,
        accessor: Arc<Accessor>,
        network_id: NetworkId,
        trace_sync: bool,
    ) -> Result<Self> {
        let notification_channel = Channel::<Notification>::unbounded();

        Ok(Self {
            inner: Arc::new(Inner {
                network_id,
                rpc: Mutex::new(rpc),
                notification_channel,
                listener_id: RwLock::new(None),
                consumer,
                accessor,
                syncer,
                shutdown: DuplexChannel::oneshot(),
                state,
                counters,
                notifier: None,
                trace_sync,
            }),
        })
    }

    pub async fn connect(&self) -> Result<()> {
        let options = ConnectOptions {
            block_async_connect: false,
            ..Default::default()
        };

        self.rpc_client().connect(Some(options)).await?;
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.rpc_client().disconnect().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> ListenerId {
        self.inner.listener_id.read().unwrap().unwrap()
    }

    pub fn network_id(&self) -> NetworkId {
        self.inner.network_id
    }

    pub fn rpc_api(&self) -> Arc<DynRpcApi> {
        self.inner.rpc.lock().unwrap().rpc_api().clone()
    }

    pub fn rpc_ctl(&self) -> RpcCtl {
        self.inner.rpc.lock().unwrap().rpc_ctl().clone()
    }

    pub fn rpc_url(&self) -> Option<String> {
        self.rpc_ctl().descriptor()
    }

    pub fn rpc_client(&self) -> Arc<KaspaRpcClient> {
        self.rpc_api()
            .clone()
            .downcast_arc::<KaspaRpcClient>()
            .expect("downcast to KaspaRpcClient")
    }

    pub fn consumer(&self) -> &Arc<dyn ConsumerT> {
        &self.inner.consumer
    }

    pub fn accessor(&self) -> &Arc<Accessor> {
        &self.inner.accessor
    }

    pub fn state(&self) -> &Arc<State> {
        &self.inner.state
    }

    pub fn syncer(&self) -> &Option<Arc<dyn SyncerT>> {
        &self.inner.syncer
    }

    pub fn counters(&self) -> &Arc<Counters> {
        &self.inner.counters
    }

    pub async fn init_state_from_server(&self) -> Result<bool> {
        let GetServerInfoResponse {
            server_version,
            network_id: server_network_id,
            has_utxo_index: _,
            is_synced,
            virtual_daa_score,
            rpc_api_version,
            rpc_api_revision,
        } = self.rpc_api().get_server_info().await?;

        let network_id = self.network_id();
        if network_id != server_network_id {
            return Err(Error::InvalidNetworkType(
                network_id.to_string(),
                server_network_id.to_string(),
            ));
        }

        if rpc_api_version > RPC_API_VERSION {
            let current = [RPC_API_VERSION, RPC_API_REVISION]
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(".");
            let connected = [rpc_api_version, rpc_api_revision]
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(".");
            return Err(Error::RpcApiVersion(current, connected));
        }

        self.state().set_current_daa_score(virtual_daa_score);

        // ------------------------------------------------------------------------------

        if let Some(syncer) = self.inner.syncer.as_ref() {
            // this means syncer was never started...
            if syncer.last_known_block().is_none() {
                warn!("no last known block hash found in the syncer");
                warn!("attempting to obtain last pruning point hash");
                let last_known_block_hash = self
                    .rpc_api()
                    .get_block_dag_info()
                    .await?
                    .pruning_point_hash;
                let blue_score = self
                    .rpc_api()
                    .get_block(last_known_block_hash, false)
                    .await?
                    .header
                    .blue_score;
                warn!("initializing syncer with last pruning point hash: {last_known_block_hash}");
                syncer.clone().spawn(BlueScoredChainBlockHash {
                    blue_score,
                    block_hash: last_known_block_hash,
                });
            }
        }

        // ------------------------------------------------------------------------------

        info!(
            "Connected to {}",
            self.rpc_url().unwrap_or("N/A".to_string())
        );
        info!("kaspad '{server_version}' on '{server_network_id}';  SYNC: {is_synced}  DAA: {virtual_daa_score}");
        // self.notify(Events::ServerStatus { server_version, is_synced, network_id, url: self.rpc_url() }).await?;

        Ok(is_synced)
    }

    async fn handle_connect_impl(&self) -> Result<()> {
        let is_node_synced = self.init_state_from_server().await?;

        if !is_node_synced {
            // This will cascade back to handle_connect which will trigger
            // disconnect.  If running with a resolver, this will result
            // in a reconnection attempt to a different synced node.
            return Err(Error::NodeNotSynced);
        }

        self.state().set_is_node_connected(true);
        self.state().set_is_node_synced(is_node_synced);

        self.register_notification_listener().await?;

        Ok(())
    }

    pub async fn handle_connect(&self) -> Result<()> {
        match self.handle_connect_impl().await {
            Err(err) => {
                error!("Error while connecting to node: {err}");
                // force disconnect the client if we have failed
                // to negotiate the connection to the node.
                // self.rpc_client().trigger_abort()?;
                self.disconnect().await?;
                task::sleep(Duration::from_secs(3)).await;
                self.connect().await?;
                Err(err)
            }
            Ok(_) => Ok(()),
        }
    }

    pub async fn handle_disconnect(&self) -> Result<()> {
        self.state().set_is_node_connected(false);
        self.unregister_notification_listener().await?;

        self.consumer().clone().disconnected()?;

        Ok(())
    }

    async fn register_notification_listener(&self) -> Result<()> {
        // println!("registering notification listener");

        let listener_id = self.rpc_api().register_new_listener(ChannelConnection::new(
            "NEXUS",
            self.inner.notification_channel.sender.clone(),
            ChannelType::Persistent,
        ));
        self.inner.listener_id.write().unwrap().replace(listener_id);

        let rpc_api = self.rpc_api();
        rpc_api
            .start_notify(
                self.listener_id(),
                Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {}),
            )
            .await?;
        rpc_api
            .start_notify(
                self.listener_id(),
                Scope::VirtualChainChanged(VirtualChainChangedScope {
                    include_accepted_transaction_ids: true,
                }),
            )
            .await?;

        Ok(())
    }

    async fn unregister_notification_listener(&self) -> Result<()> {
        let listener_id = self.inner.listener_id.write().unwrap().take();
        // we do not need this as we are unregister the entire listener here...
        if let Some(listener_id) = listener_id {
            self.rpc_api().unregister_listener(listener_id).await?;
        }

        Ok(())
    }

    async fn handle_virtual_chain_changed(
        &self,
        notification: VirtualChainChangedNotification,
    ) -> Result<()> {
        self.counters()
            .vcc_notifications
            .fetch_add(1, Ordering::Relaxed);

        self.inner
            .consumer
            .clone()
            .handle_virtual_chain_changed(notification)?;
        Ok(())
    }

    async fn handle_notification(&self, notification: Notification) -> Result<()> {
        if self.inner.trace_sync {
            info!("{:?}", notification);
        }
        match notification {
            Notification::VirtualDaaScoreChanged(virtual_daa_score_changed_notification) => {
                let VirtualDaaScoreChangedNotification { virtual_daa_score } =
                    virtual_daa_score_changed_notification;
                self.handle_daa_score_change(virtual_daa_score)?;
            }

            Notification::VirtualChainChanged(virtual_chain_changed_notification) => {
                self.handle_virtual_chain_changed(virtual_chain_changed_notification)
                    .await?;
            }

            _ => {
                log_warn!("unknown notification: {:?}", notification);
            }
        }

        Ok(())
    }

    pub fn handle_daa_score_change(&self, current_daa_score: u64) -> Result<()> {
        self.state().set_current_daa_score(current_daa_score);

        Ok(())
    }

    async fn task(self: Arc<Self>) -> Result<()> {
        let rpc_ctl_channel = self.rpc_ctl().multiplexer().channel();
        let notification_receiver = self.inner.notification_channel.receiver.clone();

        loop {
            select_biased! {
                msg = rpc_ctl_channel.receiver.recv().fuse() => {
                    match msg {
                        Ok(msg) => {
                            // handle RPC channel connection and disconnection events
                            match msg {
                                RpcState::Connected => {
                                    if !self.state().is_node_connected() {
                                        if let Err(err) = self.handle_connect().await {
                                            error!("Nexus sync task error: {err}");
                                            println!("+Nexus sync task error: {err}");
                                        }
                                    }
                                },
                                RpcState::Disconnected => {
                                    if self.state().is_node_connected() {
                                        self.handle_disconnect().await.unwrap_or_else(|err| error!("{err}"));
                                    } else {
                                        error!("NEXUS disconnected from {:?}", self.rpc_url());
                                        println!("+NEXUS disconnected from {:?}", self.rpc_url());
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            panic!("Nexus RpcCtl channel error: {err}");
                        }
                    }
                }
                notification = notification_receiver.recv().fuse() => {
                    match notification {
                        Ok(notification) => {
                            if let Err(err) = self.handle_notification(notification).await {
                                error!("error while handling notification: {err}");
                            }
                        }
                        Err(err) => {
                            panic!("RPC notification channel error: {err}");
                        }
                    }
                },

                // we use select_biased to drain rpc_ctl
                // and notifications before shutting down
                // as such task_ctl is last in the poll order
                _ = self.inner.shutdown.request.recv().fuse() => {
                    break;
                },

            }
        }

        // handle power down on rpc channel that remains connected
        if self.state().is_node_connected() {
            self.handle_disconnect()
                .await
                .unwrap_or_else(|err| log_error!("{err}"));
        }

        self.inner.shutdown.response.send(()).await?;

        Ok(())
    }

    // --------------------------------------------------------
    // -  ____ ___  ____    _  _ ____ ___ _  _ ____ ___  ____
    // -  |__/ |__] |       |\/| |___  |  |__| |  | |  \ [__
    // -  |  \ |    |___    |  | |___  |  |  | |__| |__/ ___]
    // --------------------------------------------------------

    #[allow(unused)]
    pub async fn subscribe_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        SubscribeRequest { subscription }: SubscribeRequest,
    ) -> Result<SubscribeResponse> {
        // println!("subscribe_call");

        // TODO - disabled for now
        // if let Some(notifier) = self.inner.notifier.as_ref() {
        //     notifier.subscribe(subscription, ctx.clone())?;
        // }

        let response = SubscribeResponse {};
        Ok(response)
    }

    // pub async fn unsubscribe_call(
    //     &self,
    //     ctx: Arc<dyn ContextT>,
    //     SubscribeRequest { subscription }: SubscribeRequest,
    // ) -> Result<SubscribeResponse> {
    //     // println!("subscribe_call");
    //     if let Some(notifier) = self.inner.notifier.as_ref() {
    //         notifier.subscribe(subscription, ctx.clone())?;
    //     }

    //     let response = SubscribeResponse {};
    //     Ok(response)
    // }

    pub async fn ping_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        _request: PingRequest,
    ) -> Result<PingResponse> {
        println!();
        println!("+------+");
        println!("| PING |");
        println!("+------+");
        println!();

        let response = PingResponse {};
        Ok(response)
    }

    pub async fn get_sync_status_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        _request: GetSyncStatusRequest,
    ) -> Result<GetSyncStatusResponse> {
        let response = GetSyncStatusResponse {
            is_node_connected: self.state().is_node_connected(),
            is_node_synced: self.state().is_node_synced(),
            is_indexer_synced: self
                .syncer()
                .as_ref()
                .map(|syncer| syncer.is_synced())
                .unwrap_or(false),
        };
        Ok(response)
    }

    pub async fn get_status_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        _request: GetStatusRequest,
    ) -> Result<GetStatusResponse> {
        let response = GetStatusResponse {
            response: self.accessor().krc721_indexer_status().await?,
        };
        Ok(response)
    }

    pub async fn get_collection_list_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetCollectionListRequest,
    ) -> Result<GetCollectionListResponse> {
        let GetCollectionListRequest { iter_args } = request;
        let list = self.accessor().krc721_collection_list(iter_args).await?;
        let response = GetCollectionListResponse { response: list };
        Ok(response)
    }

    pub async fn get_collection_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetCollectionRequest,
    ) -> Result<GetCollectionResponse> {
        let GetCollectionRequest { args } = request;
        let response = self.accessor().krc721_collection_lookup(args).await?;
        let response = GetCollectionResponse { response };
        Ok(response)
    }

    pub async fn get_token_list_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetTokenListRequest,
    ) -> Result<GetTokenListResponse> {
        let GetTokenListRequest { args, iter_args } = request;
        let response = self.accessor().krc721_token_list(args, iter_args).await?;
        let response = GetTokenListResponse { response };
        Ok(response)
    }

    pub async fn get_token_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetTokenRequest,
    ) -> Result<GetTokenResponse> {
        let GetTokenRequest { args } = request;
        let response = self.accessor().krc721_token_lookup(args).await?;
        let response = GetTokenResponse { response };
        Ok(response)
    }

    pub async fn get_address_list_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetAddressListRequest,
    ) -> Result<GetAddressListResponse> {
        let GetAddressListRequest { args, iter_args } = request;
        let response = self
            .accessor()
            .krc721_address_nft_list(args, iter_args)
            .await?;
        let response = GetAddressListResponse { response };
        Ok(response)
    }

    pub async fn get_address_lookup_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetAddressLookupRequest,
    ) -> Result<GetAddressLookupResponse> {
        let GetAddressLookupRequest { args, iter_args } = request;
        let response = self
            .accessor()
            .krc721_address_nft_lookup(args, iter_args)
            .await?;
        let response = GetAddressLookupResponse { response };
        Ok(response)
    }

    pub async fn get_op_list_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetOpListRequest,
    ) -> Result<GetOpListResponse> {
        let GetOpListRequest { iter_args } = request;
        let response = self.accessor().krc721_op_list(iter_args).await?;
        let response = GetOpListResponse { response };
        Ok(response)
    }

    pub async fn get_op_by_score_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetOpByScoreRequest,
    ) -> Result<GetOpByScoreResponse> {
        let GetOpByScoreRequest { args } = request;
        let response = self.accessor().krc721_op_by_score(args).await?;
        let response = GetOpByScoreResponse { response };
        Ok(response)
    }

    pub async fn get_op_by_txid_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetOpByTxidRequest,
    ) -> Result<GetOpByTxidResponse> {
        let GetOpByTxidRequest { args } = request;
        let response = self.accessor().krc721_op_by_txid(args).await?;
        let response = GetOpByTxidResponse { response };
        Ok(response)
    }

    pub async fn get_deployment_list_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetDeploymentListRequest,
    ) -> Result<GetDeploymentListResponse> {
        let GetDeploymentListRequest { iter_args } = request;
        let response = self.accessor().krc721_deployment_list(iter_args).await?;
        let response = GetDeploymentListResponse { response };
        Ok(response)
    }

    pub async fn get_royalty_fee_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetRoyaltyFeeRequest,
    ) -> Result<GetRoyaltyFeeResponse> {
        let GetRoyaltyFeeRequest { args } = request;
        let response = self.accessor().krc721_royalty_fee(args).await?;
        let response = GetRoyaltyFeeResponse { response };
        Ok(response)
    }

    pub async fn get_rejection_by_txid_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        request: GetRejectionByTxidRequest,
    ) -> Result<GetRejectionByTxidResponse> {
        let GetRejectionByTxidRequest { args } = request;
        let response = self.accessor().krc721_rejection_by_txid(args).await?;
        let response = GetRejectionByTxidResponse { response };
        Ok(response)
    }

    pub async fn get_reserved_tokens_call(
        &self,
        _ctx: Arc<dyn ContextT>,
        _request: GetReservedTokensRequest,
    ) -> Result<GetReservedTokensResponse> {
        let response = self.accessor().krc721_reserved_tokens().await?;
        let response = GetReservedTokensResponse { response };
        Ok(response)
    }
}

const SERVICE: &str = "NEXUS";

#[async_trait]
impl Service for Nexus {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        info!("starting NEXUS service");
        // log_trace!("starting {SERVICE}...");

        self.connect()
            .await
            .inspect(|_| info!("wRPC RK client connected"))
            .map_err(|err| ServiceError::custom(format!("{SERVICE} RPC connect error: {err}")))?;
        let span = tracing::Span::current();
        task::spawn(
            async move {
                self.task()
                    .instrument(info_span!("NEXUS task"))
                    .await
                    .unwrap_or_else(|err| error!("{SERVICE} error: {err}"));
            }
            .instrument(span),
        );

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        self.inner
            .syncer
            .as_ref()
            .inspect(|syncer| syncer.shutdown());
        // log_trace!("sending an exit signal to {SERVICE}");
        self.inner.shutdown.request.try_send(()).unwrap();
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        self.inner.shutdown.response.recv().await?;

        Ok(())
    }
}
