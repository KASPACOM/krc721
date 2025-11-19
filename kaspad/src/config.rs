use crate::imports::*;

#[derive(Debug, Clone)]
pub struct Config {
    network: Network,
    utxo_index: bool,
    enable_upnp: bool,
    enable_wrpc_borsh: bool,
    #[allow(dead_code)]
    enable_wrpc_json: bool,
    kaspad_daemon_storage_folder: Option<String>,
    memory_scale: Option<f64>,
    retention_period_days: Option<u32>,
}

impl Config {
    pub fn new(network: Network) -> Self {
        Self {
            network,
            utxo_index: false,
            enable_upnp: false,
            enable_wrpc_borsh: true,
            enable_wrpc_json: false,
            kaspad_daemon_storage_folder: None,
            memory_scale: None,
            retention_period_days: None,
        }
    }

    pub fn with_storage_folder<P: AsRef<Path>>(mut self, folder: P) -> Self {
        self.kaspad_daemon_storage_folder = Some(folder.as_ref().to_string_lossy().to_string());
        self
    }

    pub fn with_utxo_index(mut self, utxo_index: bool) -> Self {
        self.utxo_index = utxo_index;
        self
    }

    pub fn with_retention_period_days(mut self, retention_period_days: Option<u32>) -> Self {
        self.retention_period_days = retention_period_days;
        self
    }
}

impl From<Config> for Vec<String> {
    fn from(config: Config) -> Self {
        let mut args = Arglist::default();

        match config.network {
            Network::Mainnet => {}
            Network::Testnet10 => {
                args.push("--testnet");
                args.push("--netsuffix=10");
            }
        }

        args.push("--perf-metrics");
        args.push("--perf-metrics-interval-sec=1");
        args.push("--yes");
        if config.utxo_index {
            args.push("--utxoindex");
        }

        if let Some(memory_scale) = config.memory_scale {
            args.push(format!("--ram-scale={memory_scale:1.2}"));
        }

        if !config.enable_upnp {
            args.push("--disable-upnp");
        }

        args.push("--nogrpc");

        if config.enable_wrpc_borsh {
            args.push(format!(
                "--rpclisten-borsh=0.0.0.0:{}",
                config.network.default_kaspa_wrpc_port()
            ));
        } else {
            args.push(format!(
                "--rpclisten-borsh=127.0.0.1:{}",
                config.network.default_kaspa_wrpc_port()
            ));
        }

        if let Some(retention_period_days) = config.retention_period_days {
            args.push(format!("--retention-period-days={retention_period_days}"));
        }

        // args.push(format!("--uacomment={}", user_agent_comment()));

        if let Some(kaspad_daemon_storage_folder) = config.kaspad_daemon_storage_folder {
            args.push(format!("--appdir={kaspad_daemon_storage_folder}"));
        }

        args.into()
    }
}

impl IntoIterator for Config {
    type Item = String;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let args: Vec<String> = self.into();
        args.into_iter()
    }
}
