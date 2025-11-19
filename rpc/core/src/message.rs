use crate::imports::*;
use krc721_core::model::krc721::*;
use std::io::{Read, Write};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscribeRequest {
    pub subscription: Subscription,
}

impl Serializer for SubscribeRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Subscription, &self.subscription, writer)?;
        Ok(())
    }
}

impl Deserializer for SubscribeRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let subscription = load!(Subscription, reader)?;
        Ok(Self { subscription })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscribeResponse {}

impl Serializer for SubscribeResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for SubscribeResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PingRequest {}

impl Serializer for PingRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for PingRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PingResponse {}

impl Serializer for PingResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for PingResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetSyncStatusRequest {}

impl Serializer for GetSyncStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSyncStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetSyncStatusResponse {
    pub is_node_connected: bool,
    pub is_node_synced: bool,
    pub is_indexer_synced: bool,
}

impl Serializer for GetSyncStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.is_node_connected, writer)?;
        store!(bool, &self.is_node_synced, writer)?;
        store!(bool, &self.is_indexer_synced, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSyncStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let is_node_connected = load!(bool, reader)?;
        let is_node_synced = load!(bool, reader)?;
        let is_indexer_synced = load!(bool, reader)?;
        Ok(Self {
            is_node_connected,
            is_node_synced,
            is_indexer_synced,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetStatusRequest {}

impl Serializer for GetStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetStatusResponse {
    pub response: Arc<IndexerStatus>,
}

impl Serializer for GetStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Arc<IndexerStatus>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let status = load!(Arc<IndexerStatus>, reader)?;
        Ok(Self { response: status })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetMetricsRequest {}

impl Serializer for GetMetricsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetMetricsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetMetricsResponse {
    // pub metrics: Metrics,
    pub some_counter: u64,
}

impl Serializer for GetMetricsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.some_counter, writer)?;
        Ok(())
    }
}

impl Deserializer for GetMetricsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let some_counter = load!(u64, reader)?;
        Ok(Self { some_counter })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetCollectionListRequest {
    pub iter_args: IteratorArgs<Score>,
}

impl Serializer for GetCollectionListRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(IteratorArgs<Score>, &self.iter_args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCollectionListRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let iter_args = load!(IteratorArgs<Score>, reader)?;
        Ok(Self { iter_args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetCollectionListResponse {
    pub response: Pagination<Vec<CollectionMetaWrapper>, Score>,
}

impl Serializer for GetCollectionListResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(
            Pagination<Vec<CollectionMetaWrapper>, Score>,
            &self.response,
            writer
        )?;
        Ok(())
    }
}

impl Deserializer for GetCollectionListResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Pagination<Vec<CollectionMetaWrapper>, Score>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetCollectionRequest {
    // pub tick: Tick,
    pub args: CollectionLookupArgs,
}

impl Serializer for GetCollectionRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(CollectionLookupArgs, &self.args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCollectionRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(CollectionLookupArgs, reader)?;
        Ok(Self { args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetCollectionResponse {
    // pub collection: NFTCollectionDetailResponse
    pub response: Option<CollectionMetaWrapper>,
}

impl Serializer for GetCollectionResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<CollectionMetaWrapper>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCollectionResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Option<CollectionMetaWrapper>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetTokenRequest {
    // pub tick: Tick,
    pub args: TokenLookupArgs,
}

impl Serializer for GetTokenRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(TokenLookupArgs, &self.args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(TokenLookupArgs, reader)?;
        Ok(Self { args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetTokenResponse {
    pub response: Option<Token>,
}

impl Serializer for GetTokenResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<Token>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Option<Token>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetTokenListRequest {
    pub args: TokenListLookupArgs,
    pub iter_args: IteratorArgs<TokenId>,
}

impl Serializer for GetTokenListRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(TokenListLookupArgs, &self.args, writer)?;
        store!(IteratorArgs<TokenId>, &self.iter_args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenListRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(TokenListLookupArgs, reader)?;
        let iter_args = load!(IteratorArgs<TokenId>, reader)?;
        Ok(Self { args, iter_args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetTokenListResponse {
    pub response: Pagination<Vec<Token>, TokenId>,
}

impl Serializer for GetTokenListResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Pagination<Vec<Token>, TokenId>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenListResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Pagination<Vec<Token>, TokenId>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAddressLookupRequest {
    pub args: AddressLookupArgs,
    pub iter_args: IteratorArgs<TokenId>,
}

impl Serializer for GetAddressLookupRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(AddressLookupArgs, &self.args, writer)?;
        store!(IteratorArgs<TokenId>, &self.iter_args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetAddressLookupRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(AddressLookupArgs, reader)?;
        let iter_args = load!(IteratorArgs<TokenId>, reader)?;
        Ok(Self { args, iter_args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAddressLookupResponse {
    pub response: Pagination<Vec<AddressNftInfo>, TokenId>,
}

impl Serializer for GetAddressLookupResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(
            Pagination<Vec<AddressNftInfo>, TokenId>,
            &self.response,
            writer
        )?;
        Ok(())
    }
}

impl Deserializer for GetAddressLookupResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Pagination<Vec<AddressNftInfo>, TokenId>, reader)?;
        Ok(Self { response })
    }
}

// ---

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAddressListRequest {
    pub args: AddressListLookupArgs,
    pub iter_args: IteratorArgs<TickTokenOffset>,
}

impl Serializer for GetAddressListRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(AddressListLookupArgs, &self.args, writer)?;
        store!(IteratorArgs<TickTokenOffset>, &self.iter_args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetAddressListRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(AddressListLookupArgs, reader)?;
        let iter_args = load!(IteratorArgs<TickTokenOffset>, reader)?;
        Ok(Self { args, iter_args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAddressListResponse {
    pub response: Pagination<Vec<AddressNftInfo>, TickTokenOffset>,
}

impl Serializer for GetAddressListResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(
            Pagination<Vec<AddressNftInfo>, TickTokenOffset>,
            &self.response,
            writer
        )?;
        Ok(())
    }
}

impl Deserializer for GetAddressListResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Pagination<Vec<AddressNftInfo>, TickTokenOffset>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetOpListRequest {
    pub iter_args: IteratorArgs<Score>,
}

impl Serializer for GetOpListRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(IteratorArgs<Score>, &self.iter_args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetOpListRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let iter_args = load!(IteratorArgs<Score>, reader)?;
        Ok(Self { iter_args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetOpListResponse {
    pub response: Pagination<Vec<OperationMetaWrapper>, Score>,
}

impl Serializer for GetOpListResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(
            Pagination<Vec<OperationMetaWrapper>, Score>,
            &self.response,
            writer
        )?;
        Ok(())
    }
}

impl Deserializer for GetOpListResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Pagination<Vec<OperationMetaWrapper>, Score>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetOpByScoreRequest {
    pub args: OpLookupArgs,
}

impl Serializer for GetOpByScoreRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(OpLookupArgs, &self.args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetOpByScoreRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(OpLookupArgs, reader)?;
        Ok(Self { args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetOpByScoreResponse {
    pub response: Option<OperationMetaWrapper>,
}

impl Serializer for GetOpByScoreResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<OperationMetaWrapper>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetOpByScoreResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Option<OperationMetaWrapper>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetOpByTxidRequest {
    pub args: TxLookupArgs,
}

impl Serializer for GetOpByTxidRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(TxLookupArgs, &self.args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetOpByTxidRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(TxLookupArgs, reader)?;
        Ok(Self { args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetOpByTxidResponse {
    pub response: Option<OperationMetaWrapper>,
}

impl Serializer for GetOpByTxidResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<OperationMetaWrapper>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetOpByTxidResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Option<OperationMetaWrapper>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetDeploymentListRequest {
    pub iter_args: IteratorArgs<Score>,
}

impl Serializer for GetDeploymentListRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(IteratorArgs<Score>, &self.iter_args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDeploymentListRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let iter_args = load!(IteratorArgs<Score>, reader)?;
        Ok(Self { iter_args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetDeploymentListResponse {
    pub response: Pagination<Vec<ScoredDeployInfoWithCommon>, Score>,
}

impl Serializer for GetDeploymentListResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(
            Pagination<Vec<ScoredDeployInfoWithCommon>, Score>,
            &self.response,
            writer
        )?;
        Ok(())
    }
}

impl Deserializer for GetDeploymentListResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Pagination<Vec<ScoredDeployInfoWithCommon>, Score>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetRoyaltyFeeRequest {
    pub args: RoyaltyFeeLookupArgs,
}

impl Serializer for GetRoyaltyFeeRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RoyaltyFeeLookupArgs, &self.args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetRoyaltyFeeRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(RoyaltyFeeLookupArgs, reader)?;
        Ok(Self { args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetRoyaltyFeeResponse {
    pub response: Option<String>,
}

impl Serializer for GetRoyaltyFeeResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<String>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetRoyaltyFeeResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Option<String>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetRejectionByTxidRequest {
    pub args: TxLookupArgs,
}

impl Serializer for GetRejectionByTxidRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(TxLookupArgs, &self.args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetRejectionByTxidRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(TxLookupArgs, reader)?;
        Ok(Self { args })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetRejectionByTxidResponse {
    pub response: Option<String>,
}

impl Serializer for GetRejectionByTxidResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<String>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetRejectionByTxidResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Option<String>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAvailableTokenIdRangesRequest {
    pub tick: Tick,
}

impl Serializer for GetAvailableTokenIdRangesRequest {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Tick, &self.tick, writer)?;
        Ok(())
    }
}

impl Deserializer for GetAvailableTokenIdRangesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let tick = load!(Tick, reader)?;
        Ok(Self { tick })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAvailableTokenIdRangesResponse {
    pub response: AvailableRanges,
}

impl Serializer for GetAvailableTokenIdRangesResponse {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(AvailableRanges, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetAvailableTokenIdRangesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(AvailableRanges, reader)?;
        Ok(Self { response })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetReservedTokensRequest {}

impl Serializer for GetReservedTokensRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetReservedTokensRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetReservedTokensResponse {
    pub response: Vec<String>,
}

impl Serializer for GetReservedTokensResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<String>, &self.response, writer)?;
        Ok(())
    }
}

impl Deserializer for GetReservedTokensResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let response = load!(Vec<String>, reader)?;
        Ok(Self { response })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GetTokenHistoryRequest {
    pub args: TokenLookupArgs,
    pub iter_args: IteratorArgs<Score>,
}

impl Serializer for GetTokenHistoryRequest {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(TokenLookupArgs, &self.args, writer)?;
        store!(IteratorArgs<Score>, &self.iter_args, writer)?;
        Ok(())
    }
}

impl Deserializer for GetTokenHistoryRequest {
    fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let args = load!(TokenLookupArgs, reader)?;
        let iter_args = load!(IteratorArgs<Score>, reader)?;
        Ok(Self { args, iter_args })
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct GetTokenHistoryResponse {
    pub history_entities: Pagination<Vec<HistoryEntity>, Score>,
}

impl Serializer for GetTokenHistoryResponse {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(
            Pagination<Vec<HistoryEntity>, Score>,
            &self.history_entities,
            writer
        )?;
        Ok(())
    }
}

impl Deserializer for GetTokenHistoryResponse {
    fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let history_entities = load!(Pagination<Vec<HistoryEntity>, Score>, reader)?;
        Ok(Self { history_entities })
    }
}
