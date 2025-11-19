use crate::imports::*;

#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize, BorshSerialize, BorshDeserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Deploy,
    Mint,
    Transfer,
    Discount,
}

impl std::fmt::Display for Op {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Op::Deploy => write!(f, "deploy"),
            Op::Mint => write!(f, "mint"),
            Op::Transfer => write!(f, "transfer"),
            Op::Discount => write!(f, "transfer"),
        }
    }
}
