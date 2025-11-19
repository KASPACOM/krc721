use crate::result::Result;
use kaspa_addresses::Prefix;
use krc721_core::model::krc721::BlueScoredChainBlockHash;
use krc721_core::runtime::Runtime;
use krc721_nexus::analyzer::Analyzer;
use std::env::{args, var};
use std::sync::Arc;
use tracing_appender::non_blocking::WorkerGuard;
use workflow_rpc::client::ConnectOptions;

const DESIRED_DAEMON_SOFT_FD_LIMIT: u64 = 16 * 1024;
const MINIMUM_DAEMON_SOFT_FD_LIMIT: u64 = 8 * 1024;

#[derive(Default)]
pub struct Server {}

impl Server {
    pub async fn run(&self) -> Result<Option<WorkerGuard>> {
        match try_set_fd_limit(DESIRED_DAEMON_SOFT_FD_LIMIT) {
            Ok(limit) => {
                if limit < MINIMUM_DAEMON_SOFT_FD_LIMIT {
                    println!();
                    println!(
                        "| Current OS file descriptor limit (soft FD limit) is set to {limit}"
                    );
                    println!("| The kaspad node requires a setting of at least {DESIRED_DAEMON_SOFT_FD_LIMIT} to operate properly.");
                    println!("| Please increase the limits using the following command:");
                    println!("| ulimit -n {DESIRED_DAEMON_SOFT_FD_LIMIT}");
                    println!();
                }
            }
            Err(err) => {
                println!();
                println!("| Unable to initialize the necessary OS file descriptor limit (soft FD limit) to: {}", err);
                println!("| The kaspad node requires a setting of at least {DESIRED_DAEMON_SOFT_FD_LIMIT} to operate properly.");
                println!();
            }
        }

        // --- Services ---

        if args().any(|arg| arg == "--daemon") || var("KASPA_DAEMON").is_ok() {
            // NOTE: logging can not be initialized here due to interference with log4rs

            use crate::panic::*;
            use kaspa_core::signals::Signals;
            use kaspa_utils::fd_budget;
            use kaspad_lib::args::Args as NodeArgs;
            use kaspad_lib::daemon::create_core;
            use std::iter::once;

            println!("Starting Rusty Kaspa daemon");

            let args =
                once("kaspad".to_string()).chain(args().skip(1).filter(|arg| arg != "--daemon")); //.collect::<Vec<String>>();
            match NodeArgs::parse(args) {
                Ok(args) => {
                    init_ungraceful_panic_handler();

                    // println!("Args: {:?}", args);

                    let fd_total_budget = fd_budget::limit()
                        - args.rpc_max_clients as i32
                        - args.inbound_limit as i32
                        - args.outbound_target as i32;
                    let (core, _) = create_core(args, fd_total_budget);
                    Arc::new(Signals::new(&core)).init();
                    core.run();
                }
                Err(err) => {
                    println!("{err}");
                    std::process::exit(1);
                }
            }

            Ok(None)
        } else {
            use crate::args::*;
            use crate::config::IndexerConfig;
            use crate::folders::*;
            use crate::panic::*;
            use kaspa_consensus_core::Hash;
            use kaspa_wallet_core::rpc::{DynRpcApi, Rpc};
            #[allow(unused_imports)]
            use kaspa_wrpc_client::prelude::{KaspaRpcClient, Resolver, WrpcEncoding};
            use krc721_cluster::prelude::{Cluster, ClusterConfig};
            use krc721_core::network::Network;
            use krc721_core::runtime::Signals;
            use krc721_core::utils::separate_bytes;
            use krc721_database::prelude::Db;
            use krc721_http_server::HttpServer;
            use krc721_kaspad::kaspad::Kaspad;
            use krc721_nexus::nft_view;
            use krc721_nexus::prelude::{
                Accessor, BridgeT, Metrics, Nexus, Processor, RpcBridge, State, Syncer, SyncerT,
            };
            use krc721_rpc_server::{WrpcOptions, WrpcService};
            use krc721_snapshot::prelude::{Generator, Progress, Receiver, Snapshot};
            use std::collections::HashMap;
            use std::str::FromStr;
            use tracing::level_filters::LevelFilter;
            use tracing::{error, info, info_span, warn, Instrument};

            let runtime = Runtime::default();

            Signals::bind(&runtime);

            init_ungraceful_panic_handler();

            let Args {
                log_level,
                log_details,
                trace_sync,
                enable_debug_mode,
                network,
                mode,
                enable_http_server,
                http_listen,
                rpc_listen,
                node_rpc,
                is_kaspa_daemon: _,
                run_kaspa_daemon,
                utxo_index,
                remote,
                dry_run,
                init_genesis,
                get_genesis,
                retention_period_days,
                daa_ecdsa_fix,
            } = Args::parse();

            let mut indexer_config = match IndexerConfig::load_config(network) {
                Ok(config) => config,
                Err(err) => {
                    error!("Error loading krc721d config: {err}");
                    std::process::exit(1);
                }
            };
            if let Some(daa_ecdsa_fix) = daa_ecdsa_fix {
                indexer_config.daa_ecdsa_fix = daa_ecdsa_fix
            }

            let folders = Folders::default();

            if log_level == LevelFilter::TRACE {
                workflow_log::set_log_level(workflow_log::LevelFilter::Trace);
            }
            krc721_core::debug::enable(enable_debug_mode);

            if get_genesis {
                println!("Getting genesis block blue score for network: {}", network);

                // let node_rpc = format!("wss://{network}.krc721.stream/kaspa/{network}/wrpc/borsh");
                // println!("Creating rpc client: {node_rpc}");

                let resolver = Resolver::default();
                let rpc_client = Arc::new(KaspaRpcClient::new_with_args(
                    WrpcEncoding::Borsh,
                    // Some(node_rpc.as_str()),
                    None,
                    Some(resolver),
                    Some(network.into()),
                    None,
                )?);

                let options = ConnectOptions::default();
                rpc_client.connect(Some(options)).await?;

                // let rpc_ctl = rpc_client.ctl().clone();
                let rpc_api: Arc<DynRpcApi> = rpc_client;
                // let rpc = Rpc::new(rpc_api.clone(), rpc_ctl);

                let sink = rpc_api.get_sink().await.unwrap().sink;
                let blue_score = rpc_api
                    .get_block(sink, false)
                    .await
                    .unwrap()
                    .header
                    .blue_score;
                println!("Genesis block blue score: {}:{}", blue_score, sink);
                info!("Genesis block blue score: {}:{}", blue_score, sink);

                return Ok(None);
            } else if let Some(init_genesis) = init_genesis {
                let (blue_score, block_hash) = init_genesis.split_once(':').unwrap();

                // indexer_config.genesis = Some(Hash::from_str(&init_genesis_hash)?);
                let block_hash = Hash::from_str(block_hash)?;
                let blue_score = u64::from_str(blue_score).unwrap();
                println!(
                    "Initializing genesis blue score: {} with hash: {}",
                    blue_score, block_hash
                );
                let genesis_block_hash = BlueScoredChainBlockHash {
                    blue_score,
                    block_hash,
                };
                let db = Arc::new(Db::try_open(folders.data.clone(), &network)?);
                db.chain_block_scores.insert(genesis_block_hash, &())?;
                println!("Genesis block hash initialized: {}", init_genesis);
                return Ok(None);
            }

            match mode {
                Mode::Cluster => {
                    let guard = crate::logs::init_logs(
                        folders.logs,
                        network,
                        log_level,
                        log_details,
                        Some("cluster"),
                    );
                    let init_span = info_span!("INIT").entered();

                    info!("Cluster starting on network: `{}`", network);

                    let cluster_config = ClusterConfig::load_config()?;

                    let cluster = Cluster::try_new(network, &cluster_config)?;
                    runtime.bind(Arc::new(cluster.clone()));

                    let http_listen = http_listen.unwrap_or(format!(
                        "localhost:{}",
                        network.default_krc721d_http_cluster_port()
                    ));

                    let http_server = HttpServer::new(
                        network,
                        Arc::new(cluster),
                        http_listen.to_string().as_str(),
                        None,
                        None,
                        None,
                    );
                    runtime.bind(Arc::new(http_server));

                    init_span.exit();
                    runtime.run().instrument(info_span!("runtime")).await?;

                    Ok(Some(guard))
                }
                Mode::Indexer => {
                    let guard = crate::logs::init_logs(
                        folders.logs,
                        network,
                        log_level,
                        log_details,
                        Some("indexer"),
                    );
                    let init_span = info_span!("KRC721D INIT").entered();

                    info!("KRC721D - starting krc-721 indexer");

                    let cluster_config = ClusterConfig::load_config()
                        .ok()
                        .map(|config| config.get(network))
                        .transpose()?
                        .flatten();
                    if cluster_config.is_none() {
                        warn!("KRC721D - no cluster config found for '{network}', proceeding with standalone operation");
                    }

                    let node_rpc = if run_kaspa_daemon {
                        info!("KRC721D starting Rusty Kaspa daemon");
                        let config = krc721_kaspad::config::Config::new(network)
                            .with_storage_folder(folders.kaspa)
                            .with_utxo_index(utxo_index)
                            .with_retention_period_days(retention_period_days.or(Some(7)));
                        let daemon = Kaspad::new(config, None, None);
                        runtime.bind(Arc::new(daemon));

                        let node_rpc =
                            format!("ws://localhost:{}", network.default_kaspa_wrpc_port());
                        info!("KRC721D using local node wRPC: {}", node_rpc);
                        Some(node_rpc)
                    } else if remote {
                        let node_rpc =
                            format!("wss://{network}.krc721.stream/kaspa/{network}/wrpc/borsh");
                        info!("KRC721D using remote node wRPC: {}", node_rpc);
                        Some(node_rpc)
                    } else {
                        node_rpc
                    };

                    // for now use the default public node infrastructure
                    // let resolver = node_rpc.is_none().then(Resolver::default);
                    let rpc_client = Arc::new(KaspaRpcClient::new_with_args(
                        WrpcEncoding::Borsh,
                        node_rpc.as_deref(),
                        // resolver,
                        None,
                        Some(network.into()),
                        None,
                    )?);

                    let rpc_ctl = rpc_client.ctl().clone();
                    let rpc_api: Arc<DynRpcApi> = rpc_client;
                    let rpc = Rpc::new(rpc_api.clone(), rpc_ctl);

                    let db = Arc::new(Db::try_open(folders.data, &network)?);

                    let metrics = Arc::new(Metrics::try_new(db.clone(), network)?);
                    let counters = metrics.counters().clone();
                    runtime.bind(metrics.clone());

                    let state = Arc::new(State::default());
                    let view = Arc::new(nft_view::DbView::new(db.clone()));
                    let bridge: Arc<dyn BridgeT> = Arc::new(RpcBridge::new(rpc_api, state.clone()));

                    let genesis_block_hash = indexer_config.genesis;

                    if genesis_block_hash.is_none() {
                        warn!(
                            "no krc721 genesis block hash config found for network: `{}`",
                            network
                        );
                    }

                    let last_known_block = {
                        let tx = db.read_tx();
                        let last_known_block_hash = db
                            .chain_block_scores
                            .last_accepted_block_rtx(&tx)?
                            .map(|v| v.block_hash);
                        if last_known_block_hash.is_none() {
                            warn!("no last known block hash found in the database");
                        }
                        last_known_block_hash
                    }
                    .or(genesis_block_hash);

                    if last_known_block.is_none() {
                        warn!("no last known block hash found for network: `{}`", network);
                    }

                    let processor =
                        Arc::new(Processor::new(db.clone(), counters.clone(), None, None));
                    runtime.bind(processor.clone());

                    let analyzer = Analyzer::new(
                        Some(db.clone()),
                        indexer_config.restricted_tokens.clone(),
                        Prefix::from(network),
                        indexer_config.restricted_protocols.clone(),
                        indexer_config.daa_ecdsa_fix,
                    );
                    let syncer = Arc::new(Syncer::new(
                        state.clone(),
                        metrics.clone(),
                        bridge,
                        processor.clone(),
                        analyzer,
                    ));

                    if let Some(last_known_block) = last_known_block {
                        syncer.clone().spawn(last_known_block);
                    }

                    let generator = Arc::new(Generator::new(
                        db.clone(),
                        network,
                        state.clone(),
                        syncer.clone(),
                        folders.snapshots.clone(),
                    ));

                    let accessor = Arc::new(Accessor::new(
                        db,
                        view,
                        state.clone(),
                        counters.clone(),
                        Some(syncer.clone()),
                        network,
                        indexer_config.restricted_protocols,
                        indexer_config
                            .restricted_tokens
                            .keys()
                            .map(|k| k.to_string())
                            .collect(),
                    ));

                    let nexus = Nexus::new(
                        // db,
                        rpc,
                        state,
                        counters,
                        syncer.clone(),
                        Some(syncer.clone()),
                        accessor.clone(),
                        network.into(),
                        trace_sync,
                        // node_rpc.as_deref(),
                        // syncer,
                    )?;
                    // runtime.bind(nexus.processor().clone());

                    runtime.bind(Arc::new(nexus.clone()));

                    let mut rpc_listen = rpc_listen.unwrap_or("0.0.0.0".to_string());
                    if !rpc_listen.contains(':') {
                        rpc_listen
                            .push_str(format!(":{}", network.default_krc721d_wrpc_port()).as_str());
                    }

                    let wrpc_options = WrpcOptions::default().listen(rpc_listen.as_str());
                    let wrpc_server = WrpcService::try_new(&nexus, wrpc_options)
                        .await
                        .expect("Unable to create wRPC service.");
                    runtime.bind(Arc::new(wrpc_server));

                    if enable_http_server {
                        info!("KRC721D - HTTP server is enabled");
                        let http_listen = http_listen.unwrap_or(format!(
                            "localhost:{}",
                            network.default_krc721d_http_port()
                        ));
                        let http_server = HttpServer::new(
                            network,
                            accessor.clone(),
                            http_listen.to_string().as_str(),
                            None,
                            None,
                            Some(generator),
                        );
                        runtime.bind(Arc::new(http_server));
                    } else {
                        warn!("KRC721D - HTTP server is disabled");
                    }

                    init_span.exit();
                    runtime.run().instrument(info_span!("runtime")).await?;

                    Ok(Some(guard))
                }
                Mode::Notifier => {
                    // let guard = crate::logs::init_logs(folders.logs, trace_log_level);
                    // let init_span = info_span!("KRC721D INIT").entered();

                    info!("KRC721D - starting in notification mode");

                    // init_span.exit();
                    // runtime.run().instrument(info_span!("runtime")).await?;

                    Ok(None)
                }
                Mode::Archive { filename } => {
                    use cliclack::*;
                    println!();
                    intro("Starting snapshot generation")?;

                    let progress_bar = progress_bar(80);
                    progress_bar.start("Snapshot ...");
                    let progress =
                        Arc::new(Progress::default().with_progress_bar(progress_bar.clone()));
                    if let Err(err) = Snapshot::default()
                        .with_progress(progress)
                        .with_database(folders.data, &network)
                        .with_archive(&filename)
                        .skip_partitions(vec!["notification_queue"])
                        .archive_database()
                        .await
                    {
                        log::error(err)?;
                        return Ok(None);
                    }

                    let size = if let Ok(metadata) = std::fs::metadata(&filename) {
                        separate_bytes(metadata.len())
                    } else {
                        "N/A".to_string()
                    };

                    progress_bar.stop(format!("Snapshot: {filename} size: {size} bytes"));
                    outro("Snapshot generation is complete")?;
                    println!();

                    Ok(None)
                }
                Mode::Restore { filename } => {
                    use cliclack::*;

                    println!();
                    intro("Starting database restore")?;
                    let progress_bar = progress_bar(80);
                    progress_bar.start("Restoring ...");
                    let progress =
                        Arc::new(Progress::default().with_progress_bar(progress_bar.clone()));
                    match Snapshot::default()
                        .with_progress(progress)
                        .with_database(folders.data, &network)
                        .with_archive(filename)
                        .restore()
                        .await
                    {
                        Ok(header) => {
                            progress_bar.stop(header.to_string().as_str());
                        }
                        Err(err) => {
                            progress_bar.stop("Error...");
                            log::error(err)?;
                            return Ok(None);
                        }
                    }
                    outro("Database restore is complete")?;
                    println!();

                    Ok(None)
                }
                Mode::Purge => {
                    use cliclack::*;

                    println!();
                    if confirm("Are you sure you want to purge the database?").interact()? {
                        log::info("Purging database")?;
                        Snapshot::default()
                            .with_database(folders.data, &network)
                            .purge()?;
                        outro("Database purge is complete")?;
                    } else {
                        log::warning("Database purge aborted")?;
                    }
                    println!();

                    Ok(None)
                }
                Mode::SyncReset { server } => {
                    use cliclack::*;

                    let Some(server) = server else {
                        log::error("Please specify an indexer URL...")?;
                        return Ok(None);
                    };

                    println!();
                    intro("Resetting remote indexer")?;

                    let receiver = Receiver::new(network, folders.sync, Default::default());
                    receiver.request_reset(server.as_str()).await?;

                    outro("Remote indexer reset is complete")?;
                    println!();

                    Ok(None)
                }
                Mode::Sync { server } => {
                    use cliclack::*;

                    println!();

                    if !dry_run {
                        let db_folder = Db::database_folder(&folders.data, &network);
                        if std::fs::exists(&db_folder)? {
                            log::error(
                                "Database exists, please purge (--purge) before syncing...",
                            )?;
                            println!();
                            return Ok(None);
                        }
                    }

                    intro("Syncing from remote indexer")?;

                    let mut servers = HashMap::new();
                    if let Some(server) = server {
                        servers.insert(network, vec![server]);
                    } else {
                        Network::iter().for_each(|network| {
                            servers.insert(network, vec![format!("http://{network}.krc721.art")]);
                        });
                    }
                    let receiver = Receiver::new(network, folders.sync, servers);
                    let filename = receiver.fetch().await?;

                    log::info(format!("Deploying snapshot: {}", filename.display()))?;

                    if dry_run {
                        log::warning("Dry run mode, skipping snapshot restore")?;
                    } else {
                        let progress_bar = progress_bar(80);
                        progress_bar.start("Restoring ...");
                        let progress =
                            Arc::new(Progress::default().with_progress_bar(progress_bar.clone()));
                        match Snapshot::default()
                            .with_progress(progress)
                            .with_database(folders.data, &network)
                            .with_archive(filename)
                            .restore()
                            .await
                        {
                            Ok(header) => {
                                progress_bar.stop(header.to_string().as_str());
                            }
                            Err(err) => {
                                progress_bar.stop("Error...");
                                log::error(err)?;
                                return Ok(None);
                            }
                        }
                    }

                    outro("Sync is complete")?;
                    println!();

                    Ok(None)
                }
            }
        }
    }
}

pub fn try_set_fd_limit(limit: u64) -> Result<u64> {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "windows")] {
            Ok(rlimit::setmaxstdio(limit as u32).map(|v| v as u64)?)
        } else if #[cfg(unix)] {
            Ok(rlimit::increase_nofile_limit(limit)?)
        }
    }
}
