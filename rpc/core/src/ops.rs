use crate::imports::*;

#[derive(
    Describe,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
#[repr(u8)]
#[borsh(use_discriminant = true)]
pub enum RpcApiOps {
    Notify = 0,
    Subscribe = 1,
    Ping = 2,
    GetSyncStatus = 4,
    GetStatus = 10,
    GetCollectionList = 11,
    GetCollection = 12,
    GetTokenList = 13,
    GetToken = 14,
    GetAddressList = 15,
    GetAddressLookup = 16,
    GetOpList = 17,
    GetOpByScore = 18,
    GetOpByTxid = 19,
    GetDeploymentList = 20,
    GetRoyaltyFee = 21,
    GetRejectionByTxid = 22,
    GetReservedTokens = 23,
    GetAvailableTokenIdRanges = 24,
    GetTokenHistory = 25,
}
