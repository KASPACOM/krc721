use crate::imports::*;

#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    Eq,
    PartialEq,
    Deserialize,
    Serialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    #[default]
    #[serde(rename = "krc-721")]
    Krc721,
}
