use crate::args::*;
use crate::imports::*;
use crate::procload::ProcLoad;
use async_std::channel::unbounded;
use kaspa_wallet_core::rpc::{DynRpcApi, Rpc};
use kaspa_wrpc_client::prelude::{KaspaRpcClient, Resolver, WrpcEncoding};
use krc721_core::model::krc721::ScoredCheckedOperation;
use krc721_core::runtime::Runtime;
use krc721_core::runtime::Signals;
use krc721_database::database::Db;
use krc721_http_server::HttpServer;
use krc721_nexus::analyzer::Analyzer;
use krc721_nexus::nft_view;
use krc721_nexus::prelude::{
    Accessor, BridgeT, Metrics, Nexus, Processor, RpcBridge, State, Syncer,
};
use std::fs;
use std::sync::Arc;
use tracing_appender::non_blocking::WorkerGuard;
use workflow_core::dirs::home_dir;

#[derive(Default)]
pub struct Server {}

impl Server {
    pub async fn run(&self) -> Result<Option<WorkerGuard>> {
        let runtime = Arc::new(Runtime::default());
        Signals::bind(&runtime);

        let Args {
            trace_log_level,
            trace_sync,
            enable_debug_mode,
            network,
            mode,
            enable_http_server,
            http_listen,
            node_rpc,
        } = Args::parse();

        let init_span = info_span!("krc721d INIT").entered();

        let home_dir = home_dir().unwrap();
        let data_dir = home_dir.join(
            mode.data_dir()
                .as_deref()
                .unwrap_or("krc721d-integration-testing"),
        );

        let logs_dir = data_dir.join("logs");

        if trace_log_level {
            workflow_log::set_log_level(workflow_log::LevelFilter::Debug);
        }

        krc721_core::debug::enable(enable_debug_mode);

        info!("krc721d Testing starting on network: `{network}`");
        if data_dir.exists() {
            info!("Removing existing database directory at {:?}", data_dir);
            match fs::remove_dir_all(&data_dir) {
                Ok(_) => info!("Successfully removed old database directory"),
                Err(e) => {
                    error!("Failed to remove database directory: {}", e);
                    return Err(e.into());
                }
            }
        }
        [&logs_dir].into_iter().for_each(|dir| {
            std::fs::create_dir_all(dir).unwrap();
        });
        let guard = crate::logs::init_logs(&logs_dir);

        info!("Integration testing using data dir: {}", data_dir.display());

        match mode {
            Mode::Playback { database } => {
                let test_db = crate::database::Db::new(database.as_deref().unwrap_or("capture"));

                let db = Arc::new(Db::try_open(data_dir, &network)?);

                let metrics = Arc::new(Metrics::try_new(db.clone(), network)?);
                let counters = metrics.counters().clone();
                runtime.bind(metrics.clone());

                let state = Arc::new(State::default());
                // let view = nft_view::DbView::new(db.clone());
                // let accessor = Arc::new(Accessor::new(view, state.clone(), metrics.clone()));
                // let _bridge: Arc<dyn BridgeT> = Arc::new(RpcBridge::new(rpc_api, state.clone()));

                let player = Arc::new(crate::player::Player::new(test_db));

                let _last_known_block = {
                    let tx = db.read_tx();
                    db.chain_block_scores
                        .last_accepted_block_rtx(&tx)?
                        .map(|v| v.block_hash)
                }
                .unwrap_or_default(); // todo replace by const??
                let (sender, _receiver) = unbounded::<ScoredCheckedOperation>();
                let (tx_write_sender, _tx_write_receiver) = unbounded::<()>();

                let processor = Arc::new(Processor::new(
                    db,
                    counters,
                    Some(sender),
                    Some(tx_write_sender),
                ));
                runtime.bind(processor.clone());
                _ = processor.send_historical_virtual_chain_changed_notification_and_apply_queue(
                    Default::default(),
                ); // todo if historical simulation is needed - don't switch
                let analyzer = Analyzer::new(
                    None,
                    Default::default(),
                    network.into(),
                    Arc::new([
                        "krc721".to_string(),
                        "kspr721".to_string(),
                        "ipfs".to_string(),
                    ]),
                    0,
                );
                let _syncer = Arc::new(Syncer::new(
                    state.clone(),
                    metrics.clone(),
                    player.clone(),
                    processor.clone(),
                    analyzer,
                ));

                // TODO - execute playback via syncer
            }

            Mode::Capture { database } => {
                info!("krc721d - starting krc-721 indexer");

                // for now use the default public node infrastructure
                let resolver = Resolver::default();
                let rpc_client = Arc::new(KaspaRpcClient::new_with_args(
                    WrpcEncoding::Borsh,
                    node_rpc.as_deref(),
                    Some(resolver),
                    Some(network.into()),
                    None,
                )?);

                let rpc_ctl = rpc_client.ctl().clone();
                let rpc_api: Arc<DynRpcApi> = rpc_client;
                let rpc = Rpc::new(rpc_api.clone(), rpc_ctl);

                let db = Arc::new(Db::try_open(data_dir, &network)?);

                let metrics = Arc::new(Metrics::try_new(db.clone(), network)?);
                let counters = metrics.counters().clone();
                runtime.bind(metrics.clone());

                let state = Arc::new(State::default());
                let view = Arc::new(nft_view::DbView::new(db.clone()));
                let accessor = Arc::new(Accessor::new(
                    db,
                    view,
                    state.clone(),
                    counters.clone(),
                    None,
                    network,
                    Default::default(),
                    Default::default(),
                ));
                let _bridge: Arc<dyn BridgeT> = Arc::new(RpcBridge::new(rpc_api, state.clone()));

                let test_db = crate::database::Db::new(database.as_deref().unwrap_or("capture"));
                let analyzer = Analyzer::new(
                    None,
                    Default::default(),
                    network.into(),
                    Arc::new([
                        "krc721".to_string(),
                        "kspr721".to_string(),
                        "ipfs".to_string(),
                    ]),
                    0,
                );
                let capture = Arc::new(crate::recorder::Recorder::new(test_db, analyzer));
                // todo!();

                // let processor =
                //     Arc::new(Processor::new(db, state.clone(), bridge, metrics.clone()));
                // runtime.bind(processor.clone());

                let nexus = Nexus::new(
                    // db,
                    rpc,
                    state,
                    counters,
                    capture,
                    None,
                    accessor.clone(),
                    network.into(),
                    trace_sync,
                )?;
                // runtime.bind(nexus.processor().clone());

                runtime.bind(Arc::new(nexus.clone()));

                // Arc::new(nexus.accessor()clone())
                // Some(nexus.accessor().clone())

                if enable_http_server {
                    info!("KRC721 - HTTP server is enabled");
                    let http_listen = http_listen
                        .unwrap_or(format!("localhost:{}", network.default_krc721d_http_port()));
                    let http_server = HttpServer::new(
                        network,
                        nexus.accessor().clone(),
                        http_listen.to_string().as_str(),
                        None,
                        None,
                        None,
                    );
                    runtime.bind(Arc::new(http_server));
                } else {
                    warn!("krc721d - HTTP server is disabled");
                }
            }
            Mode::Procload {
                database: _,
                config,
            } => {
                let db = Arc::new(Db::try_open(&data_dir, &network)?);

                let metrics = Arc::new(Metrics::try_new(db.clone(), network)?);
                let counters = metrics.counters().clone();
                runtime.bind(metrics.clone());

                let (sender, receiver) = unbounded::<ScoredCheckedOperation>();
                let (tx_write_sender, tx_write_receiver) = unbounded::<()>();
                let processor = Arc::new(Processor::new(
                    db.clone(),
                    counters,
                    Some(sender),
                    Some(tx_write_sender),
                ));
                runtime.bind(processor.clone());
                _ = processor.send_historical_virtual_chain_changed_notification_and_apply_queue(
                    Default::default(),
                ); // todo if historical simulation is needed - don't switch

                runtime.bind(Arc::new(ProcLoad::new(
                    runtime.clone(),
                    processor,
                    db,
                    config.unwrap_or_default(), // Use provided config or default
                    receiver,
                    tx_write_receiver,
                    metrics,
                )));
            }
        }

        init_span.exit();
        runtime.run().instrument(info_span!("runtime")).await?;

        Ok(Some(guard))
    }
}
