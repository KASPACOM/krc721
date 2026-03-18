use kaspa_consensus_core::tx::TransactionId;

use crate::imports::*;
use crate::model::krc721::database::*;
use crate::model::krc721::model::*;
use crate::model::krc721::tick::*;
use std::fmt;

// ----------------------------------------------------------------
// -  ___  ____    _  _ ____ ___    ____ _  _ ____ _  _ ____ ____
// -  |  \ |  |    |\ | |  |  |     |    |__| |__| |\ | | __ |___
// -  |__/ |__|    | \| |__|  |     |___ |  | |  | | \| |__] |___
// -
// ----------------------------------------------------------------

#[derive(
    Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, Hash,
)]
pub struct IteratorArgs<Offset = Score> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<Offset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Pagination<T, Offset> {
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_offset: Option<Offset>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, Hash)]
pub struct TickTokenOffset {
    pub tick: Tick,
    pub token_id: u64,
}

impl<'de> Deserialize<'de> for TickTokenOffset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TickTokenOffsetVisitor;

        impl serde::de::Visitor<'_> for TickTokenOffsetVisitor {
            type Value = TickTokenOffset;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string in format 'tick-token_idx'")
            }

            fn visit_str<E>(self, value: &str) -> Result<TickTokenOffset, E>
            where
                E: serde::de::Error,
            {
                let Some((tick, id)) = value.split_once('-') else {
                    return Err(E::custom("expected format: tick-token_idx"));
                };

                let tick = tick.parse().map_err(|_| E::custom("invalid tick"))?;
                let token_idx = id.parse().map_err(|_| E::custom("invalid token_id"))?;
                Ok(TickTokenOffset {
                    tick,
                    token_id: token_idx,
                })
            }
        }

        deserializer.deserialize_str(TickTokenOffsetVisitor)
    }
}

impl serde::Serialize for TickTokenOffset {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = format!("{}-{}", self.tick, self.token_id);
        serializer.serialize_str(&s)
    }
}

pub type Score = u64;

#[derive(
    Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, Hash,
)]
pub struct CollectionLookupArgs {
    pub tick: Tick,
}

#[derive(
    Debug,
    Deserialize,
    Serialize,
    BorshSerialize,
    BorshDeserialize,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Copy,
)]
pub struct TokenListLookupArgs {
    pub tick: Tick,
}

#[derive(
    Debug,
    Deserialize,
    Serialize,
    BorshSerialize,
    BorshDeserialize,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Copy,
)]
pub struct TokenLookupArgs {
    pub tick: Tick,
    pub id: u64,
}

#[derive(
    Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, Hash,
)]
pub struct AddressLookupArgs {
    pub tick: Tick,
    pub address: String,
}

#[derive(
    Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, Hash,
)]
pub struct AddressListLookupArgs {
    pub address: String,
}

#[derive(
    Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, Hash,
)]
pub struct OpLookupArgs {
    pub score: u64,
}

#[derive(
    Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, Hash,
)]
pub struct TxLookupArgs {
    pub txid: TransactionId,
}

#[derive(
    Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize, Clone, PartialEq, Eq, Hash,
)]
pub struct RoyaltyFeeLookupArgs {
    pub address: String,
    pub tick: Tick,
}

pub type CoreError = crate::error::Error;
pub type CoreResult<T> = std::result::Result<T, CoreError>;

pub type TokenId = u64;

#[async_trait]
pub trait DataT: Send + Sync {
    async fn krc721_indexer_status(&self) -> CoreResult<Arc<IndexerStatus>>;

    async fn krc721_collection_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<CollectionMetaWrapper>, Score>>;

    async fn krc721_collection_lookup(
        &self,
        args: CollectionLookupArgs,
    ) -> CoreResult<Option<CollectionMetaWrapper>>;

    async fn krc721_token_list(
        &self,
        args: TokenListLookupArgs,
        iter_args: IteratorArgs<TokenId>,
    ) -> CoreResult<Pagination<Vec<Token>, TokenId>>;

    async fn krc721_token_lookup(&self, args: TokenLookupArgs) -> CoreResult<Option<Token>>;

    async fn krc721_address_nft_list(
        &self,
        args: AddressListLookupArgs,
        iter_args: IteratorArgs<TickTokenOffset>,
    ) -> CoreResult<Pagination<Vec<AddressNftInfo>, TickTokenOffset>>;

    async fn krc721_address_nft_lookup(
        &self,
        args: AddressLookupArgs,
        iter_args: IteratorArgs<TokenId>,
    ) -> CoreResult<Pagination<Vec<AddressNftInfo>, TokenId>>;

    async fn krc721_op_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<OperationMetaWrapper>, Score>>;

    async fn krc721_op_by_score(
        &self,
        args: OpLookupArgs,
    ) -> CoreResult<Option<OperationMetaWrapper>>;

    async fn krc721_op_by_txid(
        &self,
        args: TxLookupArgs,
    ) -> CoreResult<Option<OperationMetaWrapper>>;

    async fn krc721_deployment_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<ScoredDeployInfoWithCommon>, Score>>;

    async fn krc721_royalty_fee(&self, args: RoyaltyFeeLookupArgs) -> CoreResult<Option<String>>;

    async fn krc721_rejection_by_txid(&self, args: TxLookupArgs) -> CoreResult<Option<String>>;

    async fn krc721_reserved_tokens(&self) -> CoreResult<Vec<String>>;

    async fn krc721_token_history(
        &self,
        args: TokenLookupArgs,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<HistoryEntity>, Score>>;

    async fn krc721_available_token_id_ranges(
        &self,
        args: TokenListLookupArgs,
    ) -> CoreResult<Option<AvailableRanges>>;

    async fn krc721_active_listings(
        &self,
        args: TokenListLookupArgs,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<ListingMetaWrapper>, Score>>;

    async fn krc721_listing_lookup(
        &self,
        args: TokenLookupArgs,
    ) -> CoreResult<Option<ListingMetaWrapper>>;

    async fn krc721_address_listings(
        &self,
        args: AddressListLookupArgs,
        iter_args: IteratorArgs<TickTokenOffset>,
    ) -> CoreResult<Pagination<Vec<ListingMetaWrapper>, TickTokenOffset>>;
}
