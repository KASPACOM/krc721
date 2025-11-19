use crate::imports::*;
use ahash::AHashMap;
use kaspa_addresses::Address;
use kaspa_consensus_core::Hash;
use kaspa_hashes::ZERO_HASH;
use kaspa_txscript::pay_to_address_script;
use krc721_core::model::krc721::Tick;
use krc721_core::network::Network;
use krc721_nexus::syncer::ReservedTokenMap;
use std::env;
use std::path::PathBuf;
use toml::Value;

#[derive(Debug)]
pub struct IndexerConfig {
    pub genesis: Option<Hash>,
    pub restricted_tokens: ReservedTokenMap,
    pub restricted_protocols: Arc<[String]>,
    pub daa_ecdsa_fix: u64,
}

impl IndexerConfig {
    /// Load and parse raw TOML into sections
    pub fn load_config(network: Network) -> Result<Self> {
        let config_path = Self::locate_config()
            .ok_or(Error::custom("Unable to locate `krc721d.toml` config file"))?;
        let toml_str = std::fs::read_to_string(config_path)?;
        Self::parse_config(&toml_str, network)
    }

    pub fn parse_config(toml_str: &str, network: Network) -> Result<Self> {
        // Parse as generic TOML value
        let value: Value = toml::from_str(toml_str)?;

        let mut genesis = None;
        let mut restricted_tokens = AHashMap::new();
        let mut restricted_protocols = Vec::new();
        let mut daa_ecdsa_fix = u64::MAX;
        for (network_str, network_value) in value.as_table().unwrap_or(&toml::map::Map::new()) {
            let Ok(network_section) = Network::from_str(network_str) else {
                continue;
            };
            if network_section != network {
                continue;
            }

            let network_table = network_value.as_table().ok_or_else(|| {
                Error::custom(format!("Invalid network section: {}", network_str))
            })?;

            // Process genesis hash
            if let Some(hash_str) = network_table.get("genesis").and_then(|v| v.as_str()) {
                let hash = Hash::from_str(hash_str)?;
                if hash != ZERO_HASH {
                    genesis = Some(hash);
                }
            }

            // Process protocols
            if let Some(protocols) = network_table.get("protocols").and_then(|v| v.as_array()) {
                for protocol in protocols {
                    if let Some(protocol_str) = protocol.as_str() {
                        if !restricted_protocols.contains(&protocol_str.to_string()) {
                            restricted_protocols.push(protocol_str.to_string());
                        }
                    }
                }
            }

            // Process restricted tokens
            if let Some(restrict_table) = network_table.get("restrict").and_then(|v| v.as_table()) {
                let default_address = Address::try_from(
                    "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqhqrxplya",
                )
                .unwrap();

                for (tick_str, address_str) in restrict_table {
                    let tick: Tick = tick_str.parse()?;
                    let script_public_key = if let Some(address_str) = address_str.as_str() {
                        let address = if address_str.is_empty() {
                            &default_address
                        } else {
                            &Address::try_from(address_str)?
                        };
                        pay_to_address_script(address)
                    } else {
                        pay_to_address_script(&default_address)
                    };
                    restricted_tokens.insert(tick, script_public_key);
                }
            }
            daa_ecdsa_fix = network_table
                .get("daa_ecdsa_fix")
                .and_then(|v| v.as_integer())
                .map(|v| v as u64)
                .expect("DAA ECDSA fix must be specified");
        }

        // If no protocols were found, default to ipfs
        if restricted_protocols.is_empty() {
            restricted_protocols.push(String::from("ipfs"));
        }

        Ok(IndexerConfig {
            genesis,
            restricted_tokens,
            restricted_protocols: Arc::from(restricted_protocols.into_boxed_slice()),
            daa_ecdsa_fix,
        })
    }

    /// Scan for `krc721d.toml` config file in the current and all parent directories
    fn locate_config() -> Option<PathBuf> {
        // Start from the current executable
        let mut current_dir = env::current_exe().ok()?;

        // Keep checking parent directories until we find the config or hit the root
        loop {
            // Try to move up to parent directory
            if !current_dir.pop() {
                // We've hit the root directory without finding the config
                return None;
            }

            let config_path = current_dir.join("krc721d.toml");
            if config_path.exists() {
                return Some(config_path);
            }
        }
    }
}

#[test]
fn test_krc721d_config() {
    let toml = r###"

    [mainnet]
    genesis = "0000000000000000000000000000000000000000000000000000000000000000"
    protocols = ["ipfs", "http"]
    daa_ecdsa_fix = 0

    [mainnet.restrict]
    KSPX = "kaspatest:qqqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszapw00vun"
    KSPR = "kaspatest:qqqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszapw00vun"
    ABC = ""

    [testnet-10]
    genesis = "0000000000000000000000000000000000000000000000000000000000000000"
    protocols = ["ipfs", "http"]
    daa_ecdsa_fix = 0
    
    [testnet-10.restrict]
    KSPX = "kaspatest:qqqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszapw00vun"
    KSPR = "kaspatest:qqqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszqgpqyqszapw00vun"
    ABC = ""

    "###;

    // Test section extraction
    let config = IndexerConfig::parse_config(toml, Network::Mainnet).unwrap();
    println!("{:?}", config);
}
