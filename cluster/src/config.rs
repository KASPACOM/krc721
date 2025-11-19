use crate::imports::*;
use krc721_core::network::Network;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename = "kebab-case")]
pub struct NodeConfig {
    pub url: String,
    pub bias: Option<f64>,
    pub poll_rate: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetworkConfig {
    pub nodes: Vec<NodeConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClusterConfig {
    pub cluster: HashMap<Network, NetworkConfig>,
}

impl ClusterConfig {
    pub fn load_config() -> Result<Self> {
        let config_path =
            Self::locate_config().ok_or(Error::custom("Unable to locate cluster config file"))?;
        let toml = std::fs::read_to_string(config_path)?;
        let config = toml::from_str::<Self>(&toml)?;
        Ok(config)
    }

    pub fn get(&self, network: Network) -> Result<Option<NetworkConfig>> {
        Ok(self.cluster.get(&network).cloned())
    }

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

            // println!("Checking directory: {:?}", current_dir);

            let config_path = current_dir.join("krc721d-cluster").join("Cluster.toml");
            if config_path.exists() {
                return Some(config_path);
            }

            let config_path = current_dir.join("Cluster.toml");
            if config_path.exists() {
                return Some(config_path);
            }
        }
    }
}

#[test]
fn test_cluster_config() {
    let toml = r###"
        [cluster.mainnet]
        nodes = [
            { url = "ws://1.2.3.4:9900", bias = 1.0 },
            { url = "ws://4.5.6.7:9900", bias = 0.5 }
        ]

        [cluster.testnet-10]
        nodes = [
            { url = "ws://1.2.3.4:9900", bias = 1.0 },
            { url = "ws://4.5.6.7:9900", bias = 0.5 }
        ]
    "###;
    let config = toml::from_str::<ClusterConfig>(toml).unwrap();
    println!("{:?}", config);
}
