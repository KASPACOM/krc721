use crate::imports::*;
use crate::model::krc721::*;
use crate::network::Network;
use itertools::Itertools;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::TransactionId;
use kaspa_txscript::extract_script_pub_key_address;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
pub enum Metadata {
    #[serde(rename = "buri")]
    Remote(String),

    #[serde(rename = "metadata")]
    Local(LocalMetadata),
}

impl Metadata {
    pub fn has_incompatible_uri_prefix<T, S>(&self, acceptable_uri_prefixes: T) -> bool
    where
        T: Deref<Target = [S]>,
        S: Deref<Target = str>,
    {
        match &self {
            Metadata::Remote(buri) => acceptable_uri_prefixes
                .iter()
                .all(|acceptable_uri_prefix| !buri.starts_with(acceptable_uri_prefix.deref())),
            Metadata::Local(LocalMetadata { image, .. }) => acceptable_uri_prefixes
                .iter()
                .all(|acceptable_uri_prefix| !image.starts_with(acceptable_uri_prefix.deref())),
        }
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Self::Remote("".to_string())
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Default,
)]
pub struct LocalMetadata {
    pub name: String,
    pub description: String,
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<Attribute>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Attribute {
    #[serde(rename = "traitType")]
    pub trait_type: String, // The name/type of the trait (e.g. "Background", "Eyes", "Rarity")
    pub value: String, // The value of the trait (e.g. "Blue", "Gold", "Rare")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "displayType")]
    display_type: Option<String>, // The display type hint (e.g. "date", "boost_percentage")
}

/// API RESPONSES

#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Response<T, Offset> {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Offset>,
}

impl<T, Offset> Response<T, Offset> {
    pub fn pagination(pagination: Pagination<T, Offset>) -> Self {
        Self {
            message: "success".to_string(),
            result: Some(pagination.data),
            next: pagination.next_page_offset,
        }
    }

    pub fn single(result: Option<T>) -> Self {
        let message = if result.is_some() {
            "success"
        } else {
            "not found"
        };
        Self {
            message: message.to_string(),
            result,
            next: None,
        }
    }

    pub fn error<E: std::fmt::Display>(err: E) -> Self {
        Self {
            message: err.to_string(),
            result: None,
            next: None,
        }
    }

    pub fn has_result(&self) -> bool {
        self.result.is_some()
    }
}

/// Indexer-related structs

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerStatus {
    pub version: String,
    pub network: Network,
    pub is_node_connected: bool,
    pub is_node_synced: bool,
    pub is_indexer_synced: bool,
    pub last_known_block_hash: Option<kaspa_consensus_core::Hash>,
    pub blue_score: u64,
    pub current_op_score: u64,
    pub daa_score: u64,
    // pub current_op_score: u64,
    pub pow_fees_total: u64,
    pub royalty_fees_total: u64,
    pub token_deployments_total: u64,
    pub token_mints_total: u64,
    pub token_transfers_total: u64,
}

/// API-related structs

#[serde_as]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Balance {
    pub tick: Tick,
    #[serde(rename = "tokenId")]
    #[serde_as(as = "DisplayFromStr")]
    pub token_id: u64,
    pub attributes: String,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
/// Wrapper struct for operation structure containing
/// additional information regarding the operation
pub struct OperationMetaWrapper {
    #[serde(rename = "p")]
    pub protocol: Protocol,
    pub deployer: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_to: Option<Address>,
    // Transfer to or Mint to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Address>,
    #[serde(flatten)]
    pub operation: CheckedOperation,
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "opScore")]
    pub op_score: u64,
    #[serde(rename = "feeRev")]
    #[serde_as(as = "DisplayFromStr")]
    pub fee: u64,
}

impl OperationMetaWrapper {
    pub fn try_from(score: u64, op: CheckedOperation, prefix: Prefix) -> Result<Self> {
        let deployer = extract_script_pub_key_address(&op.operation.common.sender, prefix)?;

        let royalty_to = match op.operation.info {
            OperationInfo::Deploy(DeployInfo {
                royalty:
                    Some(RoyaltyDetails {
                        ref beneficiary, ..
                    }),
                ..
            }) => Some(extract_script_pub_key_address(beneficiary, prefix)?),
            _ => None,
        };

        let to = match op.operation.info {
            OperationInfo::Transfer(TransferInfo { ref to, .. }) => {
                Some(extract_script_pub_key_address(to, prefix)?)
            }
            OperationInfo::Mint(MintInfo { ref to, .. }) => {
                Some(extract_script_pub_key_address(to, prefix)?)
            }
            _ => None,
        };

        Ok(Self {
            fee: op.operation.common.fee,
            operation: op,
            protocol: Default::default(),
            deployer,
            royalty_to,
            to,
            op_score: score,
        })
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ScoredDeployInfoWithCommon {
    pub deployer: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_to: Option<Address>,

    #[serde(flatten)]
    pub deploy_info_with_common: DeployInfoWithCommon,

    #[serde(rename = "opScore")]
    pub op_score: u64,
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct AvailableRange {
    pub start_token_id: u64,
    pub size: u64,
}

impl FromStr for AvailableRange {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let (start_token_id, size) = s.split_once(',').ok_or(Error::custom("missing comma"))?;
        let start_token_id = start_token_id.parse().map_err(Error::custom)?;
        let size = size.parse().map_err(Error::custom)?;
        Ok(AvailableRange {
            start_token_id,
            size,
        })
    }
}

impl Serialize for AvailableRange {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&format_args!("{},{}", self.start_token_id, self.size))
    }
}

impl<'de> Deserialize<'de> for AvailableRange {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        let (start_token_id, size) = s
            .split_once(',')
            .ok_or(serde::de::Error::custom("missing comma"))?;
        let start_token_id = start_token_id.parse().map_err(serde::de::Error::custom)?;
        let size = size.parse().map_err(serde::de::Error::custom)?;
        Ok(AvailableRange {
            start_token_id,
            size,
        })
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub enum AvailableRanges {
    FullyMinted,
    Available(Vec<AvailableRange>),
}

impl Serialize for AvailableRanges {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AvailableRanges::FullyMinted => serializer.serialize_str(""),
            AvailableRanges::Available(ranges) => {
                // Calculate required capacity:
                // For each range we need:
                // - log10(start_token_id) + 1 digits for the first number
                // - log10(size) + 1 digits for the second number
                // - 2 commas (one between numbers, one between ranges)
                // Subtract 1 at the end to remove last trailing comma
                let capacity = ranges
                    .iter()
                    .map(|range| {
                        range.start_token_id.ilog10() as usize
                            + 1
                            + range.size.ilog10() as usize
                            + 1
                            + 2
                    })
                    .sum::<usize>()
                    .saturating_sub(1);
                let mut result = String::with_capacity(capacity);

                for (i, range) in ranges.iter().enumerate() {
                    if i > 0 {
                        result.push(',');
                    }
                    result.push_str(&range.start_token_id.to_string());
                    result.push(',');
                    result.push_str(&range.size.to_string());
                }
                debug_assert_eq!(capacity, result.len());
                serializer.serialize_str(&result)
            }
        }
    }
}

impl<'de> Deserialize<'de> for AvailableRanges {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        match s {
            "" => Ok(AvailableRanges::FullyMinted),
            s => {
                if s.is_empty() {
                    return Err(serde::de::Error::custom("empty string, expected range"));
                }

                let ranges = s
                    .split(',')
                    .tuples()
                    .map(|(start, size)| {
                        let start = start.parse().map_err(serde::de::Error::custom)?;
                        let size = size.parse().map_err(serde::de::Error::custom)?;
                        Ok(AvailableRange {
                            start_token_id: start,
                            size,
                        })
                    })
                    .collect::<Result<Vec<_>, D::Error>>()?;

                Ok(AvailableRanges::Available(ranges))
            }
        }
    }
}

impl ScoredDeployInfoWithCommon {
    pub fn try_from(
        score: u64,
        deploy_info_with_common: DeployInfoWithCommon,
        prefix: Prefix,
    ) -> Result<Self> {
        let deployer =
            extract_script_pub_key_address(&deploy_info_with_common.common.sender, prefix)?;

        let royalty_to = match deploy_info_with_common.info.royalty {
            Some(RoyaltyDetails {
                ref beneficiary, ..
            }) => Some(extract_script_pub_key_address(beneficiary, prefix)?),
            _ => None,
        };

        Ok(Self {
            deployer,
            royalty_to,
            deploy_info_with_common,
            op_score: score,
        })
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Collection {
    #[serde(flatten)]
    pub deploy_info_with_common: DeployInfoWithCommon,
    #[serde_as(as = "DisplayFromStr")]
    pub minted: u64,
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "opScoreMod")]
    pub op_score_modified: u64,
    pub state: CollectionState,
    #[serde(rename = "mtsMod")]
    #[serde_as(as = "DisplayFromStr")]
    pub mts_mod: u64,
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "opScoreAdd")]
    pub op_score_added: u64,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CollectionMetaWrapper {
    pub deployer: Address,
    #[serde(rename = "royaltyTo")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_to: Option<Address>,
    #[serde(flatten)]
    pub collection: Collection,
}

impl CollectionMetaWrapper {
    pub fn try_from(collection: Collection, prefix: Prefix) -> Result<Self> {
        let deployer = extract_script_pub_key_address(
            &collection.deploy_info_with_common.info.deployer,
            prefix,
        )?;

        let royalty_to = match collection.deploy_info_with_common.info.royalty {
            Some(RoyaltyDetails {
                ref beneficiary, ..
            }) => Some(extract_script_pub_key_address(beneficiary, prefix)?),
            _ => None,
        };

        Ok(Self {
            deployer,
            royalty_to,
            collection,
        })
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct HistoryEntity {
    pub owner: String,
    #[serde(rename = "opScoreMod")]
    #[serde_as(as = "DisplayFromStr")]
    pub op_score_modified: u64,
    #[serde(rename = "txIdRev")]
    pub tx_id: TransactionId,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Owner {
    pub address: String,
    #[serde_as(as = "DisplayFromStr")]
    pub id: u64,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Token {
    pub tick: Tick,
    #[serde(rename = "tokenId")]
    #[serde_as(as = "DisplayFromStr")]
    pub token_id: u64,
    pub owner: Address,
    #[serde(rename = "opScoreMod")]
    #[serde_as(as = "DisplayFromStr")]
    pub op_score_modified: u64,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AddressNftInfo {
    pub tick: Tick,
    #[serde(flatten)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_metadata: Option<Metadata>,
    #[serde(rename = "tokenId")]
    #[serde_as(as = "DisplayFromStr")]
    pub token_id: u64,
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "opScoreMod")]
    pub op_score_modified: u64,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserOperation {
    #[serde(rename = "p")]
    pub protocol: Protocol,
    pub op: Op,
    pub tick: Tick,
    #[serde(default)]
    #[serde(rename = "tokenId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub token_id: Option<u64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(flatten)]
    pub metadata: Option<Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub max: Option<u64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default)]
    #[serde(rename = "royaltyTo")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub royalty_to: Option<String>,
    #[serde(default)]
    #[serde(rename = "royaltyFee")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub royalty_fee: Option<u64>,
    #[serde(rename = "daaMintStart")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub daa_mint_start: Option<u64>,
    #[serde(rename = "discountFee")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub discount_fee: Option<u64>,
    #[serde(default)]
    #[serde(rename = "premint")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub premint: Option<u64>,
}

impl UserOperation {
    pub fn try_new(
        protocol: Protocol,
        op: Op,
        tick: impl std::fmt::Display,
    ) -> Result<Self, Error> {
        Ok(Self {
            protocol,
            op,
            tick: Tick::try_from(tick.to_string())?,
            token_id: None,
            metadata: None,
            max: None,
            to: None,
            royalty_to: None,
            royalty_fee: None,
            daa_mint_start: None,
            discount_fee: None,
            premint: None,
        })
    }

    // Builder methods
    pub fn with_token_id(mut self, token_id: u64) -> Self {
        self.token_id = Some(token_id);
        self
    }

    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_max(mut self, max: u64) -> Self {
        self.max = Some(max);
        self
    }

    pub fn with_to(mut self, to: impl Into<String>) -> Self {
        self.to = Some(to.into());
        self
    }

    pub fn with_royalty(mut self, to: impl Into<String>, fee: u64) -> Self {
        self.royalty_to = Some(to.into());
        self.royalty_fee = Some(fee);
        self
    }

    pub fn with_daa_mint_start(mut self, daa: u64) -> Self {
        self.daa_mint_start = Some(daa);
        self
    }

    pub fn with_discount_fee(mut self, fee: u64) -> Self {
        self.discount_fee = Some(fee);
        self
    }
}

impl Collection {
    pub fn fake() -> Collection {
        Collection {
            deploy_info_with_common: DeployInfoWithCommon {
                info: DeployInfo {
                    metadata: Metadata::Remote("krc721://kaspart/images/".to_string()),
                    max: 6,
                    deployer: Default::default(),
                    royalty: None,
                    mint_start_daa: 9,
                    premint: 0,
                },
                common: OperationCommon {
                    tick: Tick::from_str("ARTSY").unwrap(),
                    tx_id: Default::default(),
                    block_time: 2,
                    sender: Default::default(),
                    fee: 7,
                    accepting_block_daa_score: 8,
                },
            },
            minted: 5,
            op_score_modified: 3,
            state: CollectionState::Deployed,
            mts_mod: 1,
            op_score_added: 4,
        }
    }
}

impl Owner {
    pub fn fake() -> Self {
        Self {
            address: "kaspa:qra0p1kuze35p37gqwuu...".to_string(),
            id: 1,
        }
    }
}

impl AddressNftInfo {
    pub fn fake() -> Self {
        Self {
            tick: "KASPART".try_into().unwrap(),
            tick_metadata: Default::default(),
            token_id: 1,
            op_score_modified: 2,
        }
    }
}

impl UserOperation {
    pub fn fake() -> Self {
        Self {
            protocol: Protocol::Krc721,
            op: Op::Deploy,
            tick: "ARTSY".try_into().unwrap(),
            token_id: None,
            metadata: Some(Metadata::Remote("krc721://kaspart/images/".to_string())),
            max: Some(1000),
            to: Some("kaspa:qqabb6cz...".to_string()),
            royalty_to: Some("kaspa:qyabb6cz...".to_string()),
            royalty_fee: Some(20000000),
            daa_mint_start: None,
            discount_fee: None,
            premint: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_addresses::Prefix;
    use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptVec};
    use kaspa_txscript::opcodes::codes::{OpCheckSig, OpData32};
    use std::iter::once;

    fn spk() -> ScriptPublicKey {
        ScriptPublicKey::new(
            0,
            ScriptVec::from_iter(once(OpData32).chain([1u8; 32]).chain(once(OpCheckSig))),
        )
    }

    #[test]
    fn serialize_nft_collection() {
        let op = CheckedOperation {
            operation: Operation {
                common: OperationCommon {
                    tick: "KASPARTY".try_into().unwrap(),
                    tx_id: Default::default(),
                    block_time: 1712808987852,
                    sender: spk(),
                    fee: 100010000,
                    accepting_block_daa_score: 123,
                },
                info: OperationInfo::Transfer(TransferInfo {
                    token_id: 1234,
                    to: spk(),
                }),
            },
            error: Some(CtxValidationError::InsufficientRoyaltyFee),
        };

        let op = OperationMetaWrapper::try_from(123, op, Prefix::Testnet).unwrap();

        let op_string = serde_json::to_string_pretty(&op).unwrap();
        println!("{}", op_string);

        let op = CheckedOperation {
            operation: Operation {
                common: OperationCommon {
                    tick: "KASPARTY".try_into().unwrap(),
                    tx_id: Default::default(),
                    block_time: 1712808987852,
                    sender: spk(),
                    fee: 100010000,
                    accepting_block_daa_score: 123,
                },
                info: OperationInfo::Mint(MintInfo {
                    token_id: 1234,
                    to: spk(),
                    royalty: None,
                }),
            },
            error: Some(CtxValidationError::InsufficientRoyaltyFee),
        };

        let op = OperationMetaWrapper::try_from(123, op, Prefix::Testnet).unwrap();
        let op_string = serde_json::to_string_pretty(&op).unwrap();
        println!("{}", op_string);

        let op = CheckedOperation {
            operation: Operation {
                common: OperationCommon {
                    tick: "KASPARTY".try_into().unwrap(),
                    tx_id: Default::default(),
                    block_time: 1712808987852,
                    sender: spk(),
                    fee: 100010000,
                    accepting_block_daa_score: 123,
                },
                info: OperationInfo::Deploy(DeployInfo {
                    deployer: spk(),
                    metadata: Metadata::Local(LocalMetadata {
                        name: "KasParty".to_string(),
                        description: "Bring NFTs to Kaspa".to_string(),
                        image:
                            "https://storage.googleapis.com/opensea-prod.appspot.com/puffs/3.png"
                                .to_string(),
                        attributes: None,
                    }),
                    max: 456,
                    royalty: None,
                    mint_start_daa: 13452,
                    premint: 0,
                }),
            },
            error: Some(CtxValidationError::InsufficientRoyaltyFee),
        };

        let op = OperationMetaWrapper::try_from(123, op, Prefix::Testnet).unwrap();

        let op_string = serde_json::to_string_pretty(&op).unwrap();
        println!("{}", op_string);

        let a = vec![AddressNftInfo {
            tick: Tick::MIN,
            tick_metadata: Some(Metadata::Local(LocalMetadata {
                name: "name".to_string(),
                description: "description".to_string(),
                image: "image".to_string(),
                attributes: Some(vec![Attribute {
                    trait_type: "trait".to_string(),
                    value: "value".to_string(),
                    display_type: Some("display_type".to_string()),
                }]),
            })),
            // tick_metadata: Metadata::Remote("ipfs:://...".to_string()),
            token_id: 45,
            op_score_modified: 36,
        }];

        let a = serde_json::to_string_pretty(&a).unwrap();
        println!("{}", a);

        // let a = Detail {
        //     tick: "Kaspa".try_into().unwrap(),
        //     tokenid: 0,
        //     owner: Address::new(Prefix::Testnet, Version::PubKey, &[1u8; 32]),
        //     metadata: Metadata::Remote("http://...".to_string()),
        // };
        // let a = serde_json::to_string_pretty(&a).unwrap();
        // println!("{}", a);

        let collection = Collection {
            deploy_info_with_common: DeployInfoWithCommon {
                info: DeployInfo {
                    metadata: Metadata::Local(LocalMetadata {
                        name: "KasParty".to_string(),
                        description: "Bring NFTs to Kaspa".to_string(),
                        image:
                            "https://storage.googleapis.com/opensea-prod.appspot.com/puffs/3.png"
                                .to_string(),
                        attributes: Some(vec![Attribute {
                            trait_type: "immutableState".to_string(),
                            value: "permanent".to_string(),
                            display_type: None,
                        }]),
                    }),
                    max: 1,
                    deployer: spk(),
                    royalty: Some(RoyaltyDetails {
                        beneficiary: spk(),
                        fee: 1300000,
                    }),
                    mint_start_daa: 525037124,
                    premint: 0,
                },
                common: OperationCommon {
                    tick: "FOOO".try_into().unwrap(),
                    tx_id: Default::default(),
                    block_time: 2,
                    sender: spk(),
                    fee: 3,
                    accepting_block_daa_score: 323,
                },
            },
            minted: 4,
            op_score_modified: 5,
            state: CollectionState::Deployed,
            mts_mod: 6,
            op_score_added: 7,
        };
        let collection = CollectionMetaWrapper::try_from(collection, Prefix::Testnet).unwrap();
        let collection = serde_json::to_string_pretty(&collection).unwrap();
        println!("{}", collection);
    }

    #[test]
    fn test_raw_operation_deserialize() {
        let json = r#"{
            "p": "krc-721",
            "op": "deploy",
            "tick": "ARTSY",
            "buri": "krc721://kaspart/images/",
            "max": "1000",
            "to": "kaspa:qqabb6cz...",
            "royaltyTo": "kaspa:qyabb6cz...",
            "royaltyFee": "20000000"
        }"#;

        let operation: UserOperation = serde_json::from_str(json).unwrap();

        assert_eq!(operation.protocol, Protocol::Krc721);
        assert_eq!(operation.op, Op::Deploy);
        assert_eq!(operation.tick, Tick::from_str("ARTSY").unwrap());
        assert_eq!(
            operation.metadata,
            Some(Metadata::Remote("krc721://kaspart/images/".to_string()))
        );
        assert_eq!(operation.max, Some(1000));
        assert_eq!(operation.to, Some("kaspa:qqabb6cz...".to_string()));
        assert_eq!(operation.royalty_to, Some("kaspa:qyabb6cz...".to_string()));
        assert_eq!(operation.royalty_fee, Some(20000000));

        let json = r#"{
            "p": "krc-721",
            "op": "deploy",
            "tick": "ARTSY",
            "metadata": {
                "image": "krc721://kaspart/images/",
                "description": "Bring NFTs to Kaspa",
                "name": "Artsy",
                "attributes": []
            },
            "max": "1000",
            "minted": "30",
            "to": "kaspa:qqabb6cz...",
            "royaltyTo": "kaspa:qyabb6cz...",
            "royaltyFee": "20000000",
            "discountFee": "20000000",
            "mintDaaScore": "525037124"
        }"#;

        let metadata = Metadata::Local(LocalMetadata {
            name: "Artsy".to_string(),
            description: "Bring NFTs to Kaspa".to_string(),
            image: "krc721://kaspart/images/".to_string(),
            attributes: Some(vec![]),
        });

        let operation: UserOperation = serde_json::from_str(json).unwrap();
        assert_eq!(operation.metadata, Some(metadata));
    }
    #[test]
    fn test_serde_ranges() {
        let range = AvailableRanges::Available(vec![
            AvailableRange {
                start_token_id: 100,
                size: 50,
            },
            AvailableRange {
                start_token_id: 200,
                size: 20,
            },
            AvailableRange {
                start_token_id: 5000,
                size: 99990,
            },
        ]);

        let json = serde_json::to_string(&range).unwrap();
        assert_eq!(json, r#""100,50,200,20,5000,99990""#);
        let decoded: AvailableRanges = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, range);

        let range = AvailableRanges::Available(vec![AvailableRange {
            start_token_id: 1,
            size: 1,
        }]);

        let json = serde_json::to_string(&range).unwrap();
        assert_eq!(json, r#""1,1""#);
        let decoded: AvailableRanges = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, range);
    }
}
