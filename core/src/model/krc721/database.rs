use crate::model::krc721::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionId};
use kaspa_rpc_core::RpcHash;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;

#[repr(u8)]
#[derive(
    Debug,
    Copy,
    Clone,
    Ord,
    PartialOrd,
    Eq,
    Hash,
    PartialEq,
    Default,
    Serialize,
    Deserialize,
    BorshDeserialize,
    BorshSerialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Forward,
    #[serde(alias = "back")]
    Backward,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RoyaltyDetails {
    // #[serde(rename = "royaltyTo")]
    #[serde(skip)]
    pub beneficiary: ScriptPublicKey,
    #[serde(rename = "royaltyFee")]
    #[serde_as(as = "DisplayFromStr")]
    pub fee: u64,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize, Default)]
pub struct DeployInfo {
    #[serde(flatten)]
    pub metadata: Metadata,
    #[serde_as(as = "DisplayFromStr")]
    pub max: u64, // NonZeroU64
    #[serde(skip)]
    pub deployer: ScriptPublicKey,
    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty: Option<RoyaltyDetails>,
    #[serde(rename = "daaMintStart")]
    #[serde_as(as = "DisplayFromStr")]
    pub mint_start_daa: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub premint: u64,
}

impl DeployInfo {
    pub fn has_incompatible_uri_prefix<T, S>(&self, acceptable_uri_prefixes: T) -> bool
    where
        T: Deref<Target = [S]>,
        S: Deref<Target = str>,
    {
        self.metadata
            .has_incompatible_uri_prefix(acceptable_uri_prefixes)
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize, Default)]
pub struct DeployInfoWithCommon {
    #[serde(flatten)]
    pub info: DeployInfo,
    #[serde(flatten)]
    pub common: OperationCommon,
}

impl DeployInfoWithCommon {
    pub fn has_incompatible_uri_prefix<T, S>(&self, acceptable_uri_prefixes: T) -> bool
    where
        T: Deref<Target = [S]>,
        S: Deref<Target = str>,
    {
        self.info
            .has_incompatible_uri_prefix(acceptable_uri_prefixes)
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
pub struct MintInfo {
    #[serde(rename = "tokenId")]
    #[serde_as(as = "DisplayFromStr")]
    pub token_id: u64,
    #[serde(skip)]
    pub to: ScriptPublicKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty: Option<RoyaltyDetails>,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
pub struct TransferInfo {
    #[serde(rename = "tokenId")]
    #[serde_as(as = "DisplayFromStr")]
    pub token_id: u64,
    #[serde(skip)]
    pub to: ScriptPublicKey,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
pub struct DiscountInfo {
    #[serde(skip)]
    pub to: ScriptPublicKey,
    #[serde_as(as = "DisplayFromStr")]
    pub fee: u64,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
pub struct ListingInfo {
    #[serde(rename = "tokenId")]
    #[serde_as(as = "DisplayFromStr")]
    pub token_id: u64,
    /// The P2SH address where the listing UTXO was sent
    #[serde(skip)]
    pub utxo_address: ScriptPublicKey,
    /// The redeem script (needed to spend the listing UTXO)
    #[serde(skip)]
    pub redeem_script: Vec<u8>,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
pub struct SendInfo {
    #[serde(rename = "tokenId")]
    #[serde_as(as = "DisplayFromStr")]
    pub token_id: u64,
    /// Payment amount from tx output[0]
    #[serde(skip)]
    pub payment_amount: u64,
    /// The buyer's address (from tx output[1]).
    /// `None` when output[1] is absent — treated as a cancel/delist by the owner.
    #[serde(skip)]
    pub buyer: Option<ScriptPublicKey>,
    /// The listing UTXO txid being spent (from input[0].previous_outpoint)
    #[serde(skip)]
    pub listing_utxo_txid: TransactionId,
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize, Default)]
pub struct OperationCommon {
    pub tick: Tick,
    #[serde(rename = "txIdRev")]
    pub tx_id: TransactionId,
    #[serde(rename = "mtsAdd")]
    #[serde_as(as = "DisplayFromStr")]
    pub block_time: u64,
    #[serde(skip)]
    pub sender: ScriptPublicKey, // deployer, minter or sender, depends on context
    #[serde(skip)]
    pub fee: u64,
    #[serde(skip)]
    pub accepting_block_daa_score: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
pub struct Operation {
    #[serde(flatten)]
    pub common: OperationCommon,
    #[serde(flatten)]
    pub info: OperationInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshDeserialize, BorshSerialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "op", content = "opData")]
pub enum OperationInfo {
    Deploy(DeployInfo),
    Mint(MintInfo),
    Transfer(TransferInfo),
    Discount(DiscountInfo),
    List(ListingInfo),
    Send(SendInfo),
}

#[repr(u8)]
#[derive(Error, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum CtxValidationError {
    #[error("Tick already exists")]
    TickExists,

    #[error("Tick is reserved")]
    TickReserved,

    #[error("Insufficient royalty fee")]
    InsufficientRoyaltyFee,

    //
    // #[error("Insufficient deploy fee")]
    // InsufficientDeployFee,
    //
    // #[error("Insufficient mint fee")]
    // InsufficientMintFee,
    #[error("Missing mandatory royalty fee payment")]
    MissingRoyaltyMintFee,

    #[error("Invalid beneficiary for royalty fee")]
    InvalidBeneficiaryForRoyaltyMintFee,

    #[error("Tick not found")]
    TickNotFound,

    #[error("Minting is finished")]
    MintingFinished,

    #[error("Token not found")]
    TokenNotFound,

    #[error("Token has different Owner than sender")]
    WrongOwner,

    #[error("Minting not started yet. Current accepting block daa score: {current_accepting_block_daa_score}, start accepting block daa score: {start_accepting_block_daa_score}")]
    MintingNotStarted {
        tick: Tick,
        current_accepting_block_daa_score: u64,
        start_accepting_block_daa_score: u64,
    },
    #[error("Tick has different Deployer than sender")]
    WrongDeployer,
    #[error("Discounted fee must be less than royalty fee")]
    DiscountFeeOverflow,

    #[error("Token is already listed for sale")]
    TokenAlreadyListed,

    #[error("Listing not found for this token")]
    ListingNotFound,

    #[error("Input does not spend the listing UTXO")]
    WrongListingUtxo,

    #[error("Token is listed and cannot be transferred directly")]
    TokenIsListed,

    #[error("Invalid listing P2SH address")]
    InvalidListingP2sh,
}

#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CheckedOperation {
    #[serde(flatten)]
    pub operation: Operation,
    #[serde(rename = "opError")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CtxValidationError>,
}

#[derive(Debug)]
pub struct ScoredCheckedOperation {
    pub opscore: u64,
    pub checked_operation: CheckedOperation,
}

#[derive(Debug, BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct VirtualChainChanges {
    pub removed_chain_block_hashes: Arc<Vec<RpcHash>>,
    pub forced_rollback_blue_score: Option<BlueScore>,
    pub mergesets: Vec<Mergeset>,
}

#[derive(Debug, BorshDeserialize, BorshSerialize, Clone)]
pub struct Mergeset {
    pub operations: Vec<MergesetOperation>,
    pub entropy: u64,
    pub blue_score: BlueScore,
    pub accepted_chain_block_hash: RpcHash,
}

#[derive(Debug, BorshDeserialize, BorshSerialize, Clone)]
pub struct MergesetOperation {
    pub block_index_within_mergeset: usize,
    pub index_within_merged_block: usize,
    pub operation: Operation,
}

pub type BlueScore = u64;

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct BlueScoredChainBlockHash {
    pub blue_score: u64,
    pub block_hash: RpcHash,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptVec};
    use kaspa_txscript::opcodes::codes::{OpCheckSig, OpData32};
    use std::iter::once;
    use std::str::FromStr;

    fn spk() -> ScriptPublicKey {
        ScriptPublicKey::new(
            0,
            ScriptVec::from_iter(once(OpData32).chain([1u8; 32]).chain(once(OpCheckSig))),
        )
    }
    #[test]
    fn test_has_incompatible_uri_prefix() {
        let deployer = spk();
        let acceptable_prefixes = ["ipfs://"];

        // Test Remote metadata
        let deploy_info_remote_compatible = DeployInfoWithCommon {
            info: DeployInfo {
                metadata: Metadata::Remote("ipfs://QmT1234...".to_string()),
                max: 1000,
                deployer: deployer.clone(),
                royalty: None,
                mint_start_daa: 0,
                premint: 0,
            },
            common: OperationCommon {
                tick: Tick::from_str("TEST").unwrap(),
                tx_id: TransactionId::default(),
                block_time: 0,
                sender: deployer.clone(),
                fee: 0,
                accepting_block_daa_score: 0,
            },
        };

        let deploy_info_remote_incompatible = DeployInfoWithCommon {
            info: DeployInfo {
                metadata: Metadata::Remote("https://example.com/metadata".to_string()),
                ..deploy_info_remote_compatible.info.clone()
            },
            common: deploy_info_remote_compatible.common.clone(),
        };

        // Test Local metadata
        let deploy_info_local_compatible = DeployInfoWithCommon {
            info: DeployInfo {
                metadata: Metadata::Local(LocalMetadata {
                    name: "Test NFT".to_string(),
                    description: "Test Description".to_string(),
                    image: "ipfs://QmT1234...".to_string(),
                    attributes: None,
                }),
                ..deploy_info_remote_compatible.info.clone()
            },
            common: deploy_info_remote_compatible.common.clone(),
        };

        // Test that compatible URIs are not flagged as incompatible
        assert!(!deploy_info_remote_compatible
            .has_incompatible_uri_prefix(acceptable_prefixes.as_slice()));
        assert!(!deploy_info_local_compatible
            .has_incompatible_uri_prefix(acceptable_prefixes.as_slice()));

        // Test that incompatible URIs are correctly flagged
        assert!(deploy_info_remote_incompatible
            .has_incompatible_uri_prefix(acceptable_prefixes.as_slice()));
    }
}
