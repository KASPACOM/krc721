# RUST Data Structures

## UserOperation

The following `UserOperation` struct can be used to represent a user operation submitted to the KRC-721 indexer.

```rust


#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpDeploy {
    #[serde(rename = "p")]
    pub protocol: String,
    pub op: Op,
    pub tick: String,
    #[serde(flatten)]
    pub metadata: Metadata,
    #[serde_as(as = "DisplayFromStr")]
    pub max: u64,
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
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub premint: Option<u64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub to: Option<String>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpMint {
    #[serde(rename = "p")]
    pub protocol: String,
    pub op: Op,
    pub tick: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpDiscount {
    #[serde(rename = "p")]
    pub protocol: String,
    pub op: Op,
    pub tick: String,
    pub to: String,
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "discountFee")]
    pub fee: u64,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpTransfer {
    #[serde(rename = "p")]
    pub protocol: String,
    pub op: Op,
    pub tick: String,
    #[serde(rename = "id")]
    #[serde_as(as = "DisplayFromStr")]
    pub tokenid: u64,
    pub to: String,
}


#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Deploy,
    Mint,
    Transfer,
    Discount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
pub enum Metadata {
    // ipfs cid (ipfs://...)
    #[serde(rename = "buri")]
    Remote(String),
    #[serde(rename = "metadata")]
    Local(LocalMetadata),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct LocalMetadata {
    pub name: String,
    pub description: String,
    // ipfs cid (ipfs://...)
    pub image: String,
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

```

To serialize this struct you need the following crates:

```toml
[dependencies]
serde = { version = "1.0.215", features = ["derive"] }
serde_json = "1.0.132"
serde_with = "3.8.1"
```
