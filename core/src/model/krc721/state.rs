use crate::imports::*;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum CollectionState {
    Deployed,
    Finished,
}
