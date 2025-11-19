use crate::imports::*;
use kaspa_rpc_core::{GetVirtualChainFromBlockResponse, RpcHash};
use kaspa_wallet_core::rpc::DynRpcApi;
use krc721_core::model::krc721::BlueScoredChainBlockHash;

#[async_trait]
pub trait BridgeT: Send + Sync + 'static {
    async fn get_historical_data(&self, from: RpcHash) -> Result<GetVirtualChainFromBlockResponse>;
    async fn get_sink(&self) -> Result<BlueScoredChainBlockHash>;
}

#[derive(Clone)]
pub struct RpcBridge {
    rpc_api: Arc<DynRpcApi>,
    #[allow(unused)] // in case this is needed
    state: Arc<State>,
}

impl RpcBridge {
    pub fn new(rpc_api: Arc<DynRpcApi>, state: Arc<State>) -> Self {
        Self { rpc_api, state }
    }
}

#[async_trait]
impl BridgeT for RpcBridge {
    async fn get_historical_data(&self, from: RpcHash) -> Result<GetVirtualChainFromBlockResponse> {
        let info = self.rpc_api.get_info().await?;
        if info.is_synced {
            Ok(self
                .rpc_api
                .get_virtual_chain_from_block(from, true)
                .await?)
        } else {
            Err(Error::NodeNotSynced)
        }
    }

    async fn get_sink(&self) -> Result<BlueScoredChainBlockHash> {
        let info = self.rpc_api.get_info().await?;
        if info.is_synced {
            let sink = self.rpc_api.get_sink().await?.sink;
            let blue_score = self.rpc_api.get_block(sink, false).await?.header.blue_score;
            Ok(BlueScoredChainBlockHash {
                blue_score,
                block_hash: sink,
            })
        } else {
            Err(Error::NodeNotSynced)
        }
    }
}
