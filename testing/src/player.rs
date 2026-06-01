use crate::database::Db;
use crate::imports::*;
use kaspa_rpc_core::{GetVirtualChainFromBlockV2Response, RpcBlock, RpcHash};
use krc721_core::model::krc721::BlueScoredChainBlockHash;
use krc721_core::runtime::{Runtime, Service, ServiceResult};
use std::time::Duration;

struct Inner {
    #[allow(unused)]
    db: Db,

    shutdown: DuplexChannel<()>,
}

#[derive(Clone)]
pub struct Player {
    #[allow(unused)]
    inner: Arc<Inner>,
}

impl Player {
    pub fn new(db: Db) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                shutdown: DuplexChannel::oneshot(),
            }),
        }
    }

    // pub fn replay(self: &Arc<Self>, _consumer : Arc<dyn ConsumerT>) {
    //     // TODO - replay from db to consumer
    // }

    async fn task(self: Arc<Self>) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            select_biased! {
                _ = interval.tick().fuse() => {
                    info!("PLAYER tick");
                    // TODO - do something
                }

                _ = self.inner.shutdown.request.recv().fuse() => {
                    break;
                },

            }
        }

        self.inner.shutdown.response.send(()).await?;

        Ok(())
    }
}

#[async_trait]
impl BridgeT for Player {
    async fn get_historical_data(
        &self,
        _from: RpcHash,
    ) -> NexusResult<GetVirtualChainFromBlockV2Response> {
        Ok(GetVirtualChainFromBlockV2Response {
            removed_chain_block_hashes: Arc::new(vec![]),
            added_chain_block_hashes: Arc::new(vec![]),
            chain_block_accepted_transactions: Arc::new(vec![]),
        })
    }

    async fn get_sink(&self) -> NexusResult<BlueScoredChainBlockHash> {
        Ok(BlueScoredChainBlockHash {
            blue_score: 0,
            block_hash: Default::default(),
        })
    }

    async fn get_block(
        &self,
        _hash: RpcHash,
        _include_transactions: bool,
    ) -> NexusResult<RpcBlock> {
        Err(krc721_nexus::error::Error::custom(
            "player bridge does not provide blocks",
        ))
    }
}

const SERVICE: &str = "PLAYER";

#[async_trait]
impl Service for Player {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        info!("starting PLAYER service");
        // log_trace!("starting {SERVICE}...");

        // self.connect()
        //     .await
        //     .inspect(|_| info!("rpc client connected"))
        //     .map_err(|err| ServiceError::custom(format!("{SERVICE} RPC connect error: {err}")))?;
        let span = tracing::Span::current();
        tokio::spawn(
            async move {
                self.task()
                    .instrument(info_span!("PLAYER task"))
                    .await
                    .unwrap_or_else(|err| error!("{SERVICE} error: {err}"));
            }
            .instrument(span),
        );

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        // log_trace!("sending an exit signal to {SERVICE}");
        self.inner.shutdown.request.try_send(()).unwrap();
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        self.inner.shutdown.response.recv().await?;

        Ok(())
    }
}
