use crate::imports::*;
use kaspa_addresses::Prefix;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::network::NetworkType;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    Eq,
    PartialEq,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Network {
    Mainnet,
    #[serde(rename = "testnet-10")]
    Testnet10,
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Mainnet => write!(f, "mainnet"),
            Network::Testnet10 => write!(f, "testnet-10"),
        }
    }
}

impl FromStr for Network {
    type Err = Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Network::Mainnet),
            "testnet-10" => Ok(Network::Testnet10),
            _ => Err(Error::NetworkId(s.to_string())),
        }
    }
}

impl TryFrom<NetworkId> for Network {
    type Error = Error;
    fn try_from(network_id: NetworkId) -> std::result::Result<Self, Self::Error> {
        let NetworkId {
            network_type,
            suffix,
        } = network_id;
        match network_type {
            NetworkType::Mainnet => Ok(Network::Mainnet),
            NetworkType::Testnet if suffix == Some(10) => Ok(Network::Testnet10),
            _ => Err(Error::NetworkId(network_id.to_string())),
        }
    }
}

impl From<Network> for NetworkId {
    fn from(network: Network) -> Self {
        match network {
            Network::Mainnet => NetworkId::new(NetworkType::Mainnet),
            Network::Testnet10 => NetworkId::with_suffix(NetworkType::Testnet, 10),
        }
    }
}

impl From<Network> for NetworkType {
    fn from(network: Network) -> Self {
        match network {
            Network::Mainnet => NetworkType::Mainnet,
            Network::Testnet10 => NetworkType::Testnet,
        }
    }
}

impl From<Network> for Prefix {
    fn from(network: Network) -> Self {
        match network {
            Network::Mainnet => Prefix::Mainnet,
            Network::Testnet10 => Prefix::Testnet,
        }
    }
}

impl Network {
    pub fn default_kaspa_wrpc_port(&self) -> u16 {
        match self {
            Network::Mainnet => 17110,
            Network::Testnet10 => 17210,
            // Network::Testnet11 => 17310,
        }
    }

    pub fn default_krc721d_wrpc_port(&self) -> u16 {
        match self {
            Network::Mainnet => 9900,
            Network::Testnet10 => 9901,
            // Network::Testnet11 => 9902,
        }
    }

    pub fn default_krc721d_http_port(&self) -> u16 {
        match self {
            Network::Mainnet => 8800,
            Network::Testnet10 => 8801,
            // Network::Testnet11 => 8802,
        }
    }

    pub fn default_krc721d_http_cluster_port(&self) -> u16 {
        match self {
            Network::Mainnet => 7700,
            Network::Testnet10 => 7701,
            // Network::Testnet11 => 7702,
        }
    }

    pub fn daa_score_per_second(&self) -> u64 {
        match self {
            Network::Mainnet => 10,
            Network::Testnet10 => 10,
        }
    }

    pub fn daa_score_per_hour(&self) -> u64 {
        self.daa_score_per_second() * 3600
    }

    pub fn iter() -> impl Iterator<Item = Network> {
        [Network::Mainnet, Network::Testnet10].into_iter()
    }
}
