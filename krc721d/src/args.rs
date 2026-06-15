use crate::imports::*;
use tracing::level_filters::LevelFilter;

#[derive(Default, Debug)]
pub enum Mode {
    #[default]
    Indexer,
    Cluster,
    Notifier,
    Archive {
        filename: String,
    },
    Restore {
        filename: String,
    },
    Sync {
        server: Option<String>,
    },
    SyncReset {
        server: Option<String>,
    },
    Purge,
}

#[derive(Debug)]
pub struct Args {
    pub log_level: LevelFilter,
    pub log_details: bool,
    pub trace_sync: bool,
    pub enable_debug_mode: bool,
    pub enable_http_server: bool,
    pub mode: Mode,
    pub http_listen: Option<String>,
    pub rpc_listen: Option<String>,
    pub network: Network,
    pub node_rpc: Option<String>,
    pub is_kaspa_daemon: bool,
    pub run_kaspa_daemon: bool,
    pub utxo_index: bool,
    pub remote: bool,
    pub dry_run: bool,
    pub get_genesis: bool,
    pub init_genesis: Option<String>,
    pub retention_period_days: Option<u32>,
    pub daa_ecdsa_fix: Option<u64>,
}

impl Args {
    #[instrument(ret(Debug))]
    pub fn parse() -> Args {
        #[allow(unused)]
        use clap::{arg, command, Arg, Command};

        let cmd = Command::new("krc721d")
            .about(format!(
                "krc721 node v{}-{} (rusty-kaspa v{})",
                crate::VERSION,
                crate::GIT_DESCRIBE,
                kaspa_wallet_core::version()
            ))
            .arg(arg!(--version "Display software version"))
            .arg(arg!(--trace "Enable trace log level"))
            .arg(arg!(--debug "Enable debug mode"))
            .arg(arg!(--details "Enable detailed logging"))
            .arg(arg!(--http "Enable HTTP server in indexer mode"))
            .arg(arg!(--cluster "Cluster load-balancing mode"))
            .arg(arg!(--notifier "Transaction notification mode"))
            .arg(arg!(--remote "Connect to a remote Kaspa node address (integrated)"))
            .arg(arg!(--daemon "Spawn as Rusty Kaspa p2p daemon").hide(true))
            .arg(arg!(--local "Spawn Rusty Kaspa p2p node daemon as a child process"))
            .arg(arg!(--purge "Erase indexer database (use with caution)"))
            .arg(arg!(--utxoindex "Enable UTXO index in the local Rusty Kaspa node"))
            .arg(
                Arg::new("dry-run")
                    .long("dry-run")
                    .value_name("dry-run")
                    .num_args(0)
                    .help("Dry run mode (applicable to sync)"),
            )
            .arg(
                Arg::new("trace-sync")
                    .long("trace-sync")
                    .value_name("trace-sync")
                    .num_args(0)
                    .help("Enable Nexus Kaspa sync logging"),
            )
            .arg(
                Arg::new("mainnet")
                    .long("mainnet")
                    .value_name("mainnet")
                    .num_args(0)
                    .help("Operate on the mainnet network"),
            )
            .arg(
                Arg::new("testnet-10")
                    .long("testnet-10")
                    .value_name("testnet-10")
                    .num_args(0)
                    .help("Operate on the testnet-10 network"),
            )
            .arg(
                Arg::new("rpc-listen")
                    .long("rpc-listen")
                    .value_name("ip[:port]")
                    .num_args(0..=1)
                    .require_equals(true)
                    .value_parser(clap::value_parser!(ContextualNetAddress))
                    .help(
                        "Interface:port to listen for wRPC connections (default: localhost:7878).",
                    ),
            )
            .arg(
                Arg::new("http-listen")
                    .long("http-listen")
                    .value_name("ip[:port]")
                    .num_args(0..=1)
                    .require_equals(true)
                    .value_parser(clap::value_parser!(ContextualNetAddress))
                    .help(
                        "Interface:port to listen for HTTP connections (default localhost:8800-8802).",
                    ),
            )
            .arg(
                Arg::new("node-rpc")
                    .long("node-rpc")
                    .value_name("ws://address[:port] or wss://address[:port]")
                    .num_args(0..=1)
                    .require_equals(true)
                    .help("wRPC URL of the node (disables resolver)."),
            )
            .arg(
                Arg::new("archive")
                    .long("archive")
                    .value_name("archive.krc721")
                    .num_args(0..=1)
                    .require_equals(true)
                    .default_missing_value("archive.krc721")
                    .help("Generate local database snapshot archive"),
            )
            .arg(
                Arg::new("restore")
                    .long("restore")
                    .value_name("archive.krc721")
                    .num_args(0..=1)
                    .require_equals(true)
                    .default_missing_value("archive.krc721")
                    .help("Restore local database from snapshot archive"),
            )
            .arg(
                Arg::new("sync")
                    .long("sync")
                    .value_name("http://address or https://address")
                    .num_args(0..=1)
                    .require_equals(true)
                    .help("Sync local database from a remote indexer"),
            )
            .arg(
                Arg::new("sync-reset")
                    .long("sync-reset")
                    .value_name("http://address or https://address")
                    .num_args(0..=1)
                    .require_equals(true)
                    .help("Reset remote indexer snapshot subsystem"),
            )
            .arg(
                Arg::new("init-genesis")
                    .long("init-genesis")
                    .value_name("blue-score:block-hash")
                    .num_args(0..=1)
                    .require_equals(true)
                    .help("Initialize the genesis block hash and blue score"),
            )
            .arg(
                Arg::new("get-genesis")
                    .long("get-genesis")
                    .num_args(0)
                    .help("Get the genesis block hash"),
            )
            .arg(
                Arg::new("retention-period-days")
                    .long("retention-period-days")
                    .value_name("retention-period-days")
                    .num_args(0..=1)
            ).arg(
            Arg::new("daa-ecdsa-fix")
                .long("daa-ecdsa-fix")
                .value_name("daa-ecdsa-fix")
                .num_args(1)
                .value_parser(clap::value_parser!(u64))
                .help("Apply DAA ECDSA fix starting from the given DAA score"),
        );

        let matches = cmd.get_matches();

        if matches.get_flag("version") {
            println!("v{}-{}", crate::VERSION, crate::GIT_DESCRIBE);
            std::process::exit(0);
        }

        let log_level = if matches.get_flag("trace") {
            LevelFilter::TRACE
        } else if matches.get_flag("debug") {
            LevelFilter::DEBUG
        } else {
            LevelFilter::INFO
        };

        let log_details = matches.get_flag("details");
        let trace_sync = matches.get_flag("trace-sync");
        let enable_debug_mode = matches.get_flag("debug");
        let remote = matches.get_flag("remote");
        let is_kaspa_daemon = matches.get_flag("daemon");
        let run_kaspa_daemon = matches.get_flag("local");
        let is_cluster = matches.get_flag("cluster");
        let is_purge = matches.get_flag("purge");
        let is_notifier = matches.get_flag("notifier");
        let dry_run = matches.get_flag("dry-run");
        let utxo_index = matches.get_flag("utxoindex");
        let daa_ecdsa_fix = matches.get_one::<u64>("daa-ecdsa-fix").copied();

        if !run_kaspa_daemon && utxo_index {
            println!("UTXO index is only supported when running Rusty Kaspa daemon");
            std::process::exit(1);
        }

        let mode = if let Some(filename) = matches.get_one::<String>("archive") {
            Mode::Archive {
                filename: filename.to_string(),
            }
        } else if let Some(filename) = matches.get_one::<String>("restore") {
            Mode::Restore {
                filename: filename.to_string(),
            }
        } else if matches.contains_id("sync") {
            Mode::Sync {
                server: matches.get_one::<String>("sync").cloned(),
            }
        } else if matches.contains_id("sync-reset") {
            Mode::SyncReset {
                server: matches.get_one::<String>("sync-reset").cloned(),
            }
        } else if is_purge {
            Mode::Purge
        } else {
            match (is_cluster, is_notifier) {
                (true, true) => {
                    println!("Cannot run cluster and notifier mode at the same time (not currently supported)");
                    std::process::exit(1);
                }
                (true, false) => Mode::Cluster,
                (false, true) => Mode::Notifier,
                (false, false) => Mode::Indexer,
            }
        };

        let network = if matches.get_flag("mainnet") {
            Network::Mainnet
        } else if matches.get_flag("testnet-10") {
            Network::Testnet10
        } else {
            println!("No network specified. Must specify `--mainnet`, `--testnet-10`, aborting...");
            std::process::exit(1);
        };

        let mut enable_http_server = matches.get_flag("http");
        if is_cluster {
            enable_http_server = true;
        }

        let http_listen = matches
            .get_one::<ContextualNetAddress>("http-listen")
            .map(ToString::to_string);

        let rpc_listen = matches
            .get_one::<ContextualNetAddress>("rpc-listen")
            .map(ToString::to_string);

        let node_rpc = matches.get_one::<String>("node-rpc").cloned();

        let init_genesis = matches.get_one::<String>("init-genesis").cloned();

        let get_genesis = matches.get_flag("get-genesis");

        if let Some(node_url) = &node_rpc {
            if remote {
                println!("Cannot use --remote and --node-rpc at the same time");
                std::process::exit(1);
            }

            if let Err(err) = kaspa_wrpc_client::KaspaRpcClient::parse_url(
                node_url.to_string(),
                WrpcEncoding::Borsh,
                network.into(),
            ) {
                println!("Invalid node-rpc URL: {}", err);
                std::process::exit(1);
            }
        }

        let retention_period_days = matches.get_one::<u32>("retention-period-days").cloned();

        Args {
            log_level,
            log_details,
            trace_sync,
            enable_debug_mode,
            mode,
            enable_http_server,
            http_listen,
            rpc_listen,
            network,
            node_rpc,
            is_kaspa_daemon,
            run_kaspa_daemon,
            utxo_index,
            remote,
            dry_run,
            get_genesis,
            init_genesis,
            retention_period_days,
            daa_ecdsa_fix,
        }
    }
}
