use crate::imports::*;
use crate::procload::{CollectionConfig, LoadTestConfig, OperationConfig, ReorgConfig};
use clap::{arg, Arg, ArgAction, Command};

#[derive(Debug)]
pub enum Mode {
    Capture {
        database: Option<String>,
    },
    Playback {
        database: Option<String>,
    },
    Procload {
        database: Option<String>,
        config: Option<LoadTestConfig>,
    },
}

impl Mode {
    pub fn data_dir(&self) -> Option<String> {
        match self {
            Mode::Capture { database } => database.clone(),
            Mode::Playback { database } => database.clone(),
            Mode::Procload { database, .. } => database.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Args {
    pub trace_log_level: bool,
    pub trace_sync: bool,
    pub enable_debug_mode: bool,
    pub enable_http_server: bool,
    pub http_listen: Option<String>,
    pub mode: Mode,
    pub network: Network,
    pub node_rpc: Option<String>,
}

impl Args {
    #[instrument(ret(Debug))]
    pub fn parse() -> Args {
        let cmd = Command::new("krc721d")
            .about(format!(
                "krc721 node integration tests v{} (rusty-kaspa v{})",
                crate::VERSION,
                kaspa_wallet_core::version()
            ))
            .args([
                arg!(--version "Display software version"),
                arg!(--trace "Enable trace log level"),
                arg!(--debug "Enable debug mode"),
                arg!(--http "Enable HTTP server in indexer mode"),
                Arg::new("trace-sync")
                    .long("trace-sync")
                    .help("Enable Nexus Kaspa sync logging")
                    .action(ArgAction::SetTrue),
                Arg::new("mainnet")
                    .long("mainnet")
                    .help("Operate on the mainnet network")
                    .action(ArgAction::SetTrue),
                Arg::new("testnet-10")
                    .long("testnet-10")
                    .help("Operate on testnet-10 network")
                    .action(ArgAction::SetTrue),
            ])
            // Collection Config Arguments
            .arg(
                Arg::new("min-size")
                    .long("min-size")
                    .value_name("SIZE")
                    .help("Minimum collection size")
                    .value_parser(clap::value_parser!(u64)),
            )
            .arg(
                Arg::new("max-size")
                    .long("max-size")
                    .value_name("SIZE")
                    .help("Maximum collection size")
                    .value_parser(clap::value_parser!(u64)),
            )
            // Operation Config Arguments
            .arg(
                Arg::new("skip-deploy")
                    .long("skip-deploy")
                    .help("Skip deploy operations")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("skip-mint")
                    .long("skip-mint")
                    .help("Skip mint operations")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("skip-transfer")
                    .long("skip-transfer")
                    .help("Skip transfer operations")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("transfers-per-deploy")
                    .long("transfers-per-deploy")
                    .value_name("COUNT")
                    .help("Number of transfers per deploy")
                    .value_parser(clap::value_parser!(u64)),
            )
            .arg(
                Arg::new("reorg-depth")
                    .long("reorg-depth")
                    .value_name("DEPTH")
                    .help("Reorg depth")
                    .value_parser(clap::value_parser!(u32)),
            )
            .arg(
                Arg::new("reorg-batch-frequency")
                    .long("reorg-batch-frequency")
                    .value_name("FREQUENCY")
                    .help("Reorg frequency (batch)")
                    .value_parser(clap::value_parser!(u32)),
            )
            // General Config Arguments
            .arg(
                Arg::new("deploy-count")
                    .long("deploy-count")
                    .value_name("COUNT")
                    .help("Number of deploys")
                    .value_parser(clap::value_parser!(usize)),
            )
            .arg(
                Arg::new("mergeset-count")
                    .long("mergeset-count")
                    .value_name("COUNT")
                    .help("Number of mergesets")
                    .value_parser(clap::value_parser!(usize)),
            )
            .arg(
                Arg::new("overfill-mint")
                    .long("overfill-mint")
                    .help("Overfill mint (note: this will make metrics invalid)")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("ops-per-mergeset")
                    .long("ops-per-mergeset")
                    .value_name("COUNT")
                    .help("Operations per mergeset")
                    .value_parser(clap::value_parser!(usize)),
            )
            .arg(
                Arg::new("max-batches")
                    .long("max-batches")
                    .value_name("COUNT")
                    .help("Maximum batches to test for (mergesets)")
                    .value_parser(clap::value_parser!(usize)),
            )
            .arg(
                Arg::new("use-local-metadata")
                    .long("use-local-metadata")
                    .help("Use local metadata")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("starting-block-time")
                    .long("starting-block-time")
                    .value_name("TIME")
                    .help("Starting block time")
                    .value_parser(clap::value_parser!(u64)),
            )
            .arg(
                Arg::new("http-listen")
                    .long("http-listen")
                    .value_name("ip[:port]")
                    .require_equals(true)
                    .value_parser(clap::value_parser!(ContextualNetAddress))
                    .help("Interface:port for HTTP connections (default: localhost:7676)"),
            )
            .arg(
                Arg::new("capture")
                    .long("capture")
                    .value_name("database")
                    .require_equals(true)
                    .default_missing_value("capture")
                    .help("Capture mode with optional database name"),
            )
            .arg(
                Arg::new("playback")
                    .long("playback")
                    .value_name("database")
                    .require_equals(true)
                    .default_missing_value("playback")
                    .help("Playback mode with optional database name"),
            )
            .arg(
                Arg::new("procload")
                    .long("procload")
                    .value_name("database")
                    .num_args(0..=1)
                    .require_equals(true)
                    .default_missing_value("procload")
                    .help("Procload mode with optional database name"),
            )
            .arg(
                Arg::new("node-rpc")
                    .long("node-rpc")
                    .value_name("ws://address[:port] or wss://address[:port]")
                    .require_equals(true)
                    .help("wRPC URL of the node (disables resolver)"),
            );

        let matches = cmd.get_matches();

        let trace_log_level = matches.get_one::<bool>("trace").cloned().unwrap_or(false);
        let trace_sync = matches
            .get_one::<bool>("trace-sync")
            .cloned()
            .unwrap_or(false);
        let enable_debug_mode = matches.get_one::<bool>("debug").cloned().unwrap_or(false);
        let http_listen = matches.get_one::<String>("http-listen").cloned();

        if matches.get_flag("version") {
            info!("v{}", crate::VERSION);
            std::process::exit(0);
        }

        // Parse mode and config if it's procload
        let mode = match (
            matches.contains_id("capture"),
            matches.contains_id("playback"),
            matches.contains_id("procload"),
        ) {
            (true, false, false) => Mode::Capture {
                database: matches.get_one::<String>("capture").cloned(),
            },
            (false, true, false) => Mode::Playback {
                database: matches.get_one::<String>("playback").cloned(),
            },
            (false, false, true) => {
                let database = matches.get_one::<String>("procload").cloned();

                // Build LoadTestConfig from command line arguments
                let config = Some(LoadTestConfig {
                    overfill_mint: matches.get_flag("overfill-mint"),
                    max_batches: matches
                        .get_one::<usize>("max-batches")
                        .cloned()
                        .map(|x| x.try_into().expect("max-batches must be a u64")),
                    collection_config: CollectionConfig {
                        min_size: matches.get_one::<u64>("min-size").cloned().unwrap_or(100),
                        max_size: matches.get_one::<u64>("max-size").cloned().unwrap_or(1000),
                    },
                    operation_config: OperationConfig {
                        include_deploy: !matches.get_flag("skip-deploy"),
                        include_mint: !matches.get_flag("skip-mint"),
                        include_transfer: !matches.get_flag("skip-transfer"),
                        transfers_per_deploy: matches
                            .get_one::<u64>("transfers-per-deploy")
                            .cloned()
                            .unwrap_or(100),
                    },
                    reorg_config: ReorgConfig {
                        reorg_depth: matches.get_one::<u32>("reorg-depth").cloned().unwrap_or(3),
                        reorg_batch_frequency: matches
                            .get_one::<u32>("reorg-batch-frequency")
                            .cloned()
                            .unwrap_or(0),
                    },
                    deploy_count: matches
                        .get_one::<usize>("deploy-count")
                        .cloned()
                        .unwrap_or(1000),
                    mergeset_count: matches
                        .get_one::<usize>("mergeset-count")
                        .cloned()
                        .unwrap_or(10),
                    ops_per_mergeset: matches
                        .get_one::<usize>("ops-per-mergeset")
                        .cloned()
                        .unwrap_or(40),
                    use_local_metadata: matches.get_flag("use-local-metadata"),
                    starting_block_time: matches
                        .get_one::<u64>("starting-block-time")
                        .cloned()
                        .unwrap_or(1736356739),
                });

                Mode::Procload { database, config }
            }
            _ => {
                error!("Please select a mode: --capture, --playback, or --procload");
                std::process::exit(1);
            }
        };

        let network = if matches.get_flag("mainnet") {
            Network::Mainnet
        } else if matches.get_flag("testnet-10") {
            Network::Testnet10
        } else {
            error!("Please select a network: --mainnet or --testnet-10");
            std::process::exit(1);
        };

        let node_rpc = matches.get_one::<String>("node-rpc").cloned();
        let enable_http_server = matches.get_one::<bool>("http").cloned().unwrap_or(false);

        if let Some(node_url) = matches.get_one::<String>("node-rpc") {
            if let Err(err) = kaspa_wrpc_client::KaspaRpcClient::parse_url(
                node_url.to_string(),
                WrpcEncoding::Borsh,
                network.into(),
            ) {
                error!("Invalid node-rpc URL: {}", err);
                std::process::exit(1);
            }
        }

        Args {
            trace_log_level,
            trace_sync,
            enable_debug_mode,
            enable_http_server,
            http_listen,
            mode,
            network,
            node_rpc,
        }
    }
}
