use crate::config::ClusterConfig;
use crate::imports::*;
use krc721_core::model::krc721::*;
use krc721_rpc_core::message::*;
use tracing::*;

struct Inner {
    pub shutdown: DuplexChannel<()>,
    #[allow(unused)]
    pub connections: Vec<Connection>,
}

#[derive(Clone)]
pub struct Cluster {
    inner: Arc<Inner>,
}

impl Cluster {
    pub fn try_new(network: Network, config: &ClusterConfig) -> Result<Self> {
        let Some(config) = config.get(network)? else {
            return Err(Error::custom(format!(
                "No cluster config found for network {network}"
            )));
        };

        let connections = config
            .nodes
            .iter()
            .map(|node_config| Connection::new(network.into(), node_config.clone()))
            .collect();

        Ok(Self {
            inner: Arc::new(Inner {
                shutdown: DuplexChannel::oneshot(),
                connections,
            }),
        })
    }

    pub fn select(&self) -> Option<ConnRef<'_>> {
        let mut connections = self
            .inner
            .connections
            .iter()
            .filter(|connection| connection.is_available())
            .collect::<Vec<_>>();
        connections.sort_by_key(|a| a.sessions());
        connections.first().map(|c| c.acquire())
    }

    async fn task(self: Arc<Self>) -> Result<()> {
        self.inner.shutdown.response.send(()).await?;
        Ok(())
    }
}

const SERVICE: &str = "CLUSTER";

#[async_trait]
impl Service for Cluster {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        // log_trace!("starting {SERVICE}...");

        let this = self.clone();
        task::spawn(async move {
            this.task()
                .await
                .unwrap_or_else(|err| error!("{SERVICE} error: {err}"));
        });

        for connection in self.inner.connections.clone().into_iter() {
            info!("Connecting to {}", connection.url());
            task::spawn(async move {
                connection
                    .task()
                    .await
                    .unwrap_or_else(|err| error!("Connection error: {err}"));
            });
        }

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        // log_trace!("sending an exit signal to {}", SERVICE);
        self.inner.shutdown.request.try_send(()).unwrap();

        for connection in self.inner.connections.clone().into_iter() {
            connection.shutdown();
        }
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        self.inner.shutdown.response.recv().await?;

        for connection in self.inner.connections.clone().into_iter() {
            connection.join().await?;
        }

        Ok(())
    }
}

#[async_trait]
impl DataT for Cluster {
    /// Get the status of the indexer
    async fn krc721_indexer_status(&self) -> CoreResult<Arc<IndexerStatus>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetStatusRequest {};
        let GetStatusResponse { response } = connection
            .as_ref()
            .get_status_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    /// Get a list of KRC-721 collections
    async fn krc721_collection_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<CollectionMetaWrapper>, Score>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetCollectionListRequest { iter_args };
        let GetCollectionListResponse { response } = connection
            .as_ref()
            .get_collection_list_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    /// Get a KRC-721 collection by tick
    async fn krc721_collection_lookup(
        &self,
        args: CollectionLookupArgs,
    ) -> CoreResult<Option<CollectionMetaWrapper>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetCollectionRequest { args };
        let GetCollectionResponse { response } = connection
            .as_ref()
            .get_collection_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    /// Get a single KRC-721 token by tick and token ID
    async fn krc721_token_lookup(&self, args: TokenLookupArgs) -> CoreResult<Option<Token>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetTokenRequest { args };
        let GetTokenResponse { response } = connection
            .as_ref()
            .get_token_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    /// Get a list of KRC-721 tokens by tick
    async fn krc721_token_list(
        &self,
        args: TokenListLookupArgs,
        iter_args: IteratorArgs<TokenId>,
    ) -> CoreResult<Pagination<Vec<Token>, TokenId>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetTokenListRequest { args, iter_args };
        let GetTokenListResponse { response } = connection
            .as_ref()
            .get_token_list_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    /// Get a list of KRC-721 tokens that an address owns
    async fn krc721_address_nft_list(
        &self,
        args: AddressListLookupArgs,
        iter_args: IteratorArgs<TickTokenOffset>,
    ) -> CoreResult<Pagination<Vec<AddressNftInfo>, TickTokenOffset>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetAddressListRequest { args, iter_args };
        let GetAddressListResponse { response } = connection
            .as_ref()
            .get_address_list_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }
    async fn krc721_address_nft_lookup(
        &self,
        args: AddressLookupArgs,
        iter_args: IteratorArgs<TokenId>,
    ) -> CoreResult<Pagination<Vec<AddressNftInfo>, TokenId>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetAddressLookupRequest { args, iter_args };
        let GetAddressLookupResponse { response } = connection
            .as_ref()
            .get_address_lookup_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    /// Retrieves a list of KRC-721 operations
    /// that occurred for a certain score
    async fn krc721_op_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<OperationMetaWrapper>, Score>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetOpListRequest { iter_args };
        let GetOpListResponse { response } = connection
            .as_ref()
            .get_op_list_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    async fn krc721_op_by_score(
        &self,
        args: OpLookupArgs,
    ) -> CoreResult<Option<OperationMetaWrapper>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetOpByScoreRequest { args };
        let GetOpByScoreResponse { response } = connection
            .as_ref()
            .get_op_by_score_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    async fn krc721_op_by_txid(
        &self,
        args: TxLookupArgs,
    ) -> CoreResult<Option<OperationMetaWrapper>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetOpByTxidRequest { args };
        let GetOpByTxidResponse { response } = connection
            .as_ref()
            .get_op_by_txid_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    async fn krc721_deployment_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<ScoredDeployInfoWithCommon>, Score>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetDeploymentListRequest { iter_args };
        let GetDeploymentListResponse { response } = connection
            .as_ref()
            .get_deployment_list_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    async fn krc721_royalty_fee(&self, args: RoyaltyFeeLookupArgs) -> CoreResult<Option<String>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetRoyaltyFeeRequest { args };
        let GetRoyaltyFeeResponse { response } = connection
            .as_ref()
            .get_royalty_fee_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    async fn krc721_rejection_by_txid(&self, args: TxLookupArgs) -> CoreResult<Option<String>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetRejectionByTxidRequest { args };
        let GetRejectionByTxidResponse { response } = connection
            .as_ref()
            .get_rejection_by_txid_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    async fn krc721_reserved_tokens(&self) -> CoreResult<Vec<String>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetReservedTokensRequest {};
        let GetReservedTokensResponse { response } = connection
            .as_ref()
            .get_reserved_tokens_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    async fn krc721_token_history(
        &self,
        args: TokenLookupArgs,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<HistoryEntity>, Score>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetTokenHistoryRequest { args, iter_args };
        let GetTokenHistoryResponse {
            history_entities: response,
        } = connection
            .as_ref()
            .get_token_history_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(response)
    }

    async fn krc721_available_token_id_ranges(
        &self,
        args: TokenListLookupArgs,
    ) -> CoreResult<Option<AvailableRanges>> {
        let connection = self.select().ok_or(CoreError::ServiceNotAvailable)?;
        let request = GetAvailableTokenIdRangesRequest { tick: args.tick };
        let GetAvailableTokenIdRangesResponse { response } = connection
            .as_ref()
            .get_available_token_id_ranges_call(request)
            .await
            .map_err(CoreError::custom)?;
        Ok(Some(response))
    }
}
