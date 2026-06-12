use crate::analyzer::{Analyzer, ContextTransaction};
use crate::bridge::BridgeT;
use crate::consumer::ConsumerT;
use crate::imports::*;
use crate::metrics::Metrics;
use crate::state::State;
use ahash::AHashMap;
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_rpc_core::{
    GetVirtualChainFromBlockV2Response, RpcBlock, RpcChainBlockAcceptedTransactions, RpcHash,
    RpcOptionalTransaction, VirtualChainChangedNotification,
};
use krc721_core::model::krc721::{
    BlueScoredChainBlockHash, Mergeset, MergesetOperation, VirtualChainChanges,
};
use tracing::{instrument, Instrument};

use krc721_core::model::krc721::Tick;
pub type ReservedTokenMap = AHashMap<Tick, ScriptPublicKey>;

const SYNC_ERROR_THRESHOLD_SECONDS: u64 = 15;
const ORIGIN_HASH_BYTE: u8 = 0xfe;

pub struct Syncer {
    // This will be used for performance / realtime metrics / counters
    // Counters that track collections, tokens, owners, etc
    // should live in a dedicated database.
    #[allow(unused)]
    metrics: Arc<Metrics>,
    bridge: Arc<dyn BridgeT>,
    is_synced: AtomicBool,

    processor: Arc<Processor>,
    last_known_block: Mutex<Option<BlueScoredChainBlockHash>>,
    target: Mutex<Option<BlueScoredChainBlockHash>>,
    last_error_timestamp: AtomicU64,
    resync_requested: AtomicBool,
    sync_task_running: AtomicBool,

    shutting_down: AtomicBool,
    analyzer: Analyzer,
}

impl SyncerT for Syncer {
    fn is_synced(&self) -> bool {
        self.is_synced.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn last_known_block(&self) -> Option<BlueScoredChainBlockHash> {
        *self.last_known_block.lock().unwrap()
    }

    fn spawn(self: Arc<Self>, last_known_block: BlueScoredChainBlockHash) {
        self.spawn_sync_task(last_known_block);
    }

    fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }
}

impl Syncer {
    pub fn new(
        _state: Arc<State>,
        metrics: Arc<Metrics>,
        bridge: Arc<dyn BridgeT>,
        processor: Arc<Processor>,
        analyzer: Analyzer,
    ) -> Self {
        Self {
            metrics,
            bridge,
            is_synced: Default::default(),
            processor,
            last_known_block: Mutex::new(None),
            target: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
            analyzer,
            last_error_timestamp: AtomicU64::new(0),
            resync_requested: AtomicBool::new(false),
            sync_task_running: AtomicBool::new(false),
        }
    }

    async fn sync_task(&self) {
        let mut sink = loop {
            let Ok(sink) = self
                .bridge
                .get_sink()
                .await
                .inspect(|sink| info!("got sink: {sink:?}"))
                .inspect_err(|e| {
                    let last_error_timestamp = self.last_error_timestamp.load(Ordering::SeqCst);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    if now - last_error_timestamp > SYNC_ERROR_THRESHOLD_SECONDS {
                        error!("Failed to get sink: {:?}", e);
                        self.last_error_timestamp.store(now, Ordering::SeqCst);
                    }
                })
            else {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            };
            break sink;
        };

        loop {
            let from = self
                .last_known_block
                .lock()
                .unwrap()
                .expect("last known block is not set");
            info!("Syncing from block: {:?}", from);

            if is_same_sync_point(from, sink) {
                info!("last known block matches sync target, state is synced");
                if self.mark_synced_if_at_current_sink().await {
                    break;
                }
                sink = self.current_sync_target_or(sink);
                continue;
            }

            if from.blue_score >= sink.blue_score {
                warn!(
                    "last known block score is at or beyond sink but hash differs; replaying from {:?} to handle reorg against {:?}",
                    from, sink
                );
            }

            let Ok(GetVirtualChainFromBlockV2Response {
                mut removed_chain_block_hashes,
                added_chain_block_hashes,
                chain_block_accepted_transactions,
            }) = self
                .bridge
                .get_historical_data(from.block_hash)
                .await
                .inspect_err(|e| error!("Failed to get historical data: {:?}", e))
            else {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            };

            let mut forced_rollback_blue_score = None;
            if from.blue_score >= sink.blue_score
                && from.block_hash != sink.block_hash
                && !removed_chain_block_hashes.contains(&from.block_hash)
            {
                warn!(
                    "historical response did not remove stale sync point {}; forcing rollback from score {}",
                    from.block_hash, from.blue_score
                );
                let mut removed = (*removed_chain_block_hashes).clone();
                removed.push(from.block_hash);
                removed_chain_block_hashes = Arc::new(removed);
                forced_rollback_blue_score = Some(from.blue_score);
            }

            if let Err(err) = validate_historical_acceptance_coverage(
                &added_chain_block_hashes,
                &chain_block_accepted_transactions,
            ) {
                error!("Historical acceptance data is incomplete: {:?}", err);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            let last_added_chain_block = self
                .last_added_chain_block(&added_chain_block_hashes)
                .await
                .inspect_err(|err| error!("Failed to resolve last added chain block: {:?}", err))
                .ok()
                .flatten();

            if last_added_chain_block.is_none()
                && removed_chain_block_hashes.is_empty()
                && !is_same_sync_point(from, sink)
            {
                warn!(
                    "historical response did not include chain changes from {:?} toward {:?}; retrying",
                    from, sink
                );
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            let target_progress = {
                let target = *self.target.lock().unwrap().get_or_insert_with(|| {
                    debug!("Setting target to sink {sink:?}");
                    sink
                });
                info!("Target: {:?}", target);
                classify_target_progress(&added_chain_block_hashes, last_added_chain_block, target)
            };

            let mergesets = match reconstruct_and_process_acceptance_data(
                &self.bridge,
                &chain_block_accepted_transactions,
                &self.analyzer,
            )
            .await
            {
                Ok(mergesets) => mergesets,
                Err(err) => {
                    error!("Failed to reconstruct acceptance data: {:?}", err);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
            };

            let notification = VirtualChainChanges {
                // who cares about that arc?? no one
                removed_chain_block_hashes,
                forced_rollback_blue_score,
                mergesets,
            };

            if target_progress == TargetProgress::Reached {
                if let Err(err) = self
                    .processor
                    .send_historical_virtual_chain_changed_notification_and_wait(notification)
                {
                    error!("Failed to send historical notification: {:?}", err);
                    continue;
                }
            } else if let Err(err) = self
                .processor
                .send_historical_virtual_chain_changed_notification_and_wait(notification)
            {
                error!("Failed to send historical notification: {:?}", err);
                continue;
            }

            let next_last_known_block = match last_added_chain_block {
                Some(block) => block,
                None => match self.processor.last_accepted_block() {
                    Ok(Some(block)) => block,
                    Ok(None) => {
                        error!("No accepted chain block remains after historical replay");
                        continue;
                    }
                    Err(err) => {
                        error!("Failed to read last accepted block after historical replay: {err}");
                        continue;
                    }
                },
            };
            if let Some(v) = self.last_known_block.lock().unwrap().as_mut() {
                *v = next_last_known_block;
                debug!(target: "last_known_block_tracking", "Last known block is updated to: {:?}", next_last_known_block);
            }

            if target_progress == TargetProgress::Obsolete {
                info!("target was bypassed by historical replay; refreshing sync target");
                sink = self.refresh_sync_target_or(sink).await;
                *self.target.lock().unwrap() = Some(sink);
                continue;
            }

            if target_progress == TargetProgress::Reached {
                info!("target is reached");
                if self.mark_synced_if_at_current_sink().await {
                    break;
                }
                sink = self.current_sync_target_or(sink);
            }
        }
    }

    async fn last_added_chain_block(
        &self,
        added_chain_block_hashes: &[RpcHash],
    ) -> Result<Option<BlueScoredChainBlockHash>> {
        let Some(block_hash) = added_chain_block_hashes.last().copied() else {
            return Ok(None);
        };
        let blue_score = self
            .bridge
            .get_block(block_hash, false)
            .await?
            .header
            .blue_score;
        Ok(Some(BlueScoredChainBlockHash {
            blue_score,
            block_hash,
        }))
    }

    async fn mark_synced_if_at_current_sink(&self) -> bool {
        let latest_sink =
            match self.bridge.get_sink().await.inspect_err(|err| {
                error!("Failed to refresh sink before marking synced: {:?}", err)
            }) {
                Ok(sink) => sink,
                Err(_) => return false,
            };

        let last_known_block = self
            .last_known_block
            .lock()
            .unwrap()
            .expect("last known block is not set");
        let resync_requested = self.resync_requested.swap(false, Ordering::SeqCst);

        if !is_same_sync_point(last_known_block, latest_sink) {
            info!(
                "sync target changed during sync; continuing from {:?} to {:?}",
                last_known_block, latest_sink
            );
            *self.target.lock().unwrap() = Some(latest_sink);
            return false;
        }

        if resync_requested {
            info!("resync was requested during historical sync; current sink already covered");
        }

        let notification = VirtualChainChanges {
            removed_chain_block_hashes: Arc::new(vec![]),
            forced_rollback_blue_score: None,
            mergesets: vec![],
        };
        if let Err(err) = self
            .processor
            .send_historical_virtual_chain_changed_notification_and_apply_queue(notification)
        {
            error!("Failed to apply queued notification after sync: {:?}", err);
            return false;
        }

        info!("state is synced");
        self.is_synced.store(true, Ordering::SeqCst);
        *self.target.lock().unwrap() = None;
        true
    }

    fn current_sync_target_or(
        &self,
        fallback: BlueScoredChainBlockHash,
    ) -> BlueScoredChainBlockHash {
        self.target.lock().unwrap().unwrap_or(fallback)
    }

    async fn refresh_sync_target_or(
        &self,
        fallback: BlueScoredChainBlockHash,
    ) -> BlueScoredChainBlockHash {
        self.bridge
            .get_sink()
            .await
            .inspect_err(|err| error!("Failed to refresh sync target: {:?}", err))
            .unwrap_or(fallback)
    }

    fn spawn_sync_task_impl(self: &Arc<Self>) {
        // this should be implemented as a Service, since it is not
        // there are sequencing issues, and we need to ensure that
        // the sync task is not restarting when we are shutting down
        if self.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }

        if !self.try_claim_sync_task() {
            debug!("sync task is already running");
            return;
        }

        info!("Spawning sync task");
        tokio::spawn({
            let this = self.clone();
            async move {
                this.sync_task().await;
                this.release_sync_task();
            }
            .instrument(tracing::info_span!("sync_task"))
        });
    }

    fn try_claim_sync_task(&self) -> bool {
        try_claim_sync_task_flag(&self.sync_task_running)
    }

    fn release_sync_task(self: &Arc<Self>) {
        self.sync_task_running.store(false, Ordering::SeqCst);
        if !self.shutting_down.load(Ordering::SeqCst) && !self.is_synced() {
            self.spawn_sync_task_impl();
        }
    }

    fn spawn_sync_task(self: &Arc<Self>, last_known_block: BlueScoredChainBlockHash) {
        let mut last_known_block_guard = self.last_known_block.lock().unwrap();
        if last_known_block_guard.is_some() {
            panic!("syncer is already initialized with last known block");
        }
        last_known_block_guard.replace(last_known_block);
        self.spawn_sync_task_impl();
    }
}

fn try_claim_sync_task_flag(sync_task_running: &AtomicBool) -> bool {
    sync_task_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

fn is_same_sync_point(from: BlueScoredChainBlockHash, sink: BlueScoredChainBlockHash) -> bool {
    from == sink
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetProgress {
    Pending,
    Reached,
    Obsolete,
}

fn classify_target_progress(
    added_chain_block_hashes: &[RpcHash],
    last_added_chain_block: Option<BlueScoredChainBlockHash>,
    target: BlueScoredChainBlockHash,
) -> TargetProgress {
    if added_chain_block_hashes.contains(&target.block_hash) {
        return TargetProgress::Reached;
    }

    match last_added_chain_block {
        Some(last_added_chain_block) if last_added_chain_block.blue_score >= target.blue_score => {
            TargetProgress::Obsolete
        }
        None => TargetProgress::Pending,
        _ => TargetProgress::Pending,
    }
}

impl ConsumerT for Syncer {
    // temporarily (or permanently?) relocated to Processor from Nexus
    // to isolate Processor data ingest from Nexus logic allowing
    // Processor to receive notifications from different sources.
    #[instrument(skip_all)]
    fn handle_virtual_chain_changed(
        self: Arc<Self>,
        VirtualChainChangedNotification {
            removed_chain_block_hashes: _,
            added_chain_block_hashes: _,
            accepted_transaction_ids: _,
        }: VirtualChainChangedNotification,
    ) -> Result<()> {
        if self.is_synced.load(std::sync::atomic::Ordering::SeqCst) {
            self.processor
                .switch_to_queue_mod()
                .map_err(|_| Error::SendError)?;
            self.is_synced.store(false, Ordering::SeqCst);
            self.spawn_sync_task_impl();
        } else {
            self.resync_requested.store(true, Ordering::SeqCst);
        }

        Ok(())
    }

    fn disconnected(self: Arc<Self>) -> Result<()> {
        info!("Disconnected event is triggered");
        self.processor
            .switch_to_queue_mod()
            .map_err(|_| Error::SendError)?;
        self.is_synced.store(false, Ordering::SeqCst);
        if self.target.lock().unwrap().is_none() {
            self.spawn_sync_task_impl();
        }
        Ok(())
    }
}

pub fn process_acceptance_data(
    chain_block_accepted_transactions: &[RpcChainBlockAcceptedTransactions],
    analyzer: &Analyzer,
) -> Vec<Mergeset> {
    let chain_blocks = chain_block_accepted_transactions
        .iter()
        .map(ReconstructedChainBlockAcceptance::from_flat_accepted_transactions)
        .collect::<Vec<_>>();
    process_reconstructed_acceptance_data(&chain_blocks, analyzer)
}

async fn reconstruct_and_process_acceptance_data(
    bridge: &Arc<dyn BridgeT>,
    chain_block_accepted_transactions: &[RpcChainBlockAcceptedTransactions],
    analyzer: &Analyzer,
) -> Result<Vec<Mergeset>> {
    let mut reconstructed = Vec::with_capacity(chain_block_accepted_transactions.len());
    let mut block_cache = AHashMap::<RpcHash, RpcBlock>::new();
    for accepted in chain_block_accepted_transactions {
        reconstructed
            .push(reconstruct_chain_block_acceptance(bridge, &mut block_cache, accepted).await?);
    }

    Ok(process_reconstructed_acceptance_data(
        &reconstructed,
        analyzer,
    ))
}

fn validate_historical_acceptance_coverage(
    added_chain_block_hashes: &[RpcHash],
    chain_block_accepted_transactions: &[RpcChainBlockAcceptedTransactions],
) -> Result<()> {
    if added_chain_block_hashes.len() != chain_block_accepted_transactions.len() {
        return Err(Error::custom(format!(
            "added chain block count {} does not match acceptance record count {}",
            added_chain_block_hashes.len(),
            chain_block_accepted_transactions.len()
        )));
    }

    for (index, (expected_hash, accepted)) in added_chain_block_hashes
        .iter()
        .zip(chain_block_accepted_transactions.iter())
        .enumerate()
    {
        let Some(actual_hash) = accepted.chain_block_header.hash else {
            return Err(Error::custom(format!(
                "missing accepted chain block hash at index {index}"
            )));
        };
        if actual_hash != *expected_hash {
            return Err(Error::custom(format!(
                "acceptance record hash mismatch at index {index}: expected {expected_hash}, got {actual_hash}"
            )));
        }
    }

    Ok(())
}

async fn reconstruct_chain_block_acceptance(
    bridge: &Arc<dyn BridgeT>,
    block_cache: &mut AHashMap<RpcHash, RpcBlock>,
    accepted: &RpcChainBlockAcceptedTransactions,
) -> Result<ReconstructedChainBlockAcceptance> {
    let accepted_chain_block_hash = accepted
        .chain_block_header
        .hash
        .ok_or_else(|| Error::custom("missing accepted chain block hash in V2 response"))?;
    let accepting_block_blue_score = accepted
        .chain_block_header
        .blue_score
        .ok_or_else(|| Error::custom("missing accepted chain block blue score in V2 response"))?;
    let accepting_block_daa_score = accepted
        .chain_block_header
        .daa_score
        .ok_or_else(|| Error::custom("missing accepted chain block DAA score in V2 response"))?;

    let accepting_block =
        get_cached_block(bridge, block_cache, accepted_chain_block_hash, false).await?;
    let verbose_data = accepting_block.verbose_data.as_ref().ok_or_else(|| {
        Error::custom(format!(
            "missing verbose data for accepted chain block {accepted_chain_block_hash}"
        ))
    })?;

    let selected_parent_hash = verbose_data.selected_parent_hash;
    let mut merged_blocks = Vec::with_capacity(
        1 + verbose_data.merge_set_blues_hashes.len() + verbose_data.merge_set_reds_hashes.len(),
    );
    let selected_parent_timestamp = if is_origin_hash(&selected_parent_hash) {
        0
    } else {
        get_cached_block(bridge, block_cache, selected_parent_hash, false)
            .await?
            .header
            .timestamp
    };
    merged_blocks.push(ReconstructedMergedBlockAcceptance {
        hash: selected_parent_hash,
        timestamp: selected_parent_timestamp,
        transactions: Vec::new(),
    });

    let mut known_merged_blocks = AHashMap::<RpcHash, ()>::new();
    known_merged_blocks.insert(selected_parent_hash, ());
    let mut sortable = Vec::new();
    for hash in verbose_data
        .merge_set_blues_hashes
        .iter()
        .copied()
        .filter(|hash| *hash != selected_parent_hash)
        .chain(verbose_data.merge_set_reds_hashes.iter().copied())
    {
        if known_merged_blocks.insert(hash, ()).is_some() {
            continue;
        }
        if is_origin_hash(&hash) {
            sortable.push((hash, Default::default(), 0));
            continue;
        }
        let block = get_cached_block(bridge, block_cache, hash, false).await?;
        sortable.push((hash, block.header.blue_work, block.header.timestamp));
    }

    for rpc_tx in &accepted.accepted_transactions {
        let Some(verbose_data) = rpc_tx.verbose_data.as_ref() else {
            return Err(Error::custom(format!(
                "accepted transaction in chain block {accepted_chain_block_hash} is missing verbose data"
            )));
        };
        let Some(block_hash) = verbose_data.block_hash else {
            return Err(Error::custom(format!(
                "accepted transaction in chain block {accepted_chain_block_hash} is missing verbose block hash"
            )));
        };
        if known_merged_blocks.insert(block_hash, ()).is_some() {
            continue;
        }
        warn!(
            "accepted transaction references block {} not listed in reconstructed mergeset for {}; adding it from transaction verbose data",
            block_hash, accepted_chain_block_hash
        );
        if is_origin_hash(&block_hash) {
            sortable.push((block_hash, Default::default(), 0));
            continue;
        }
        let block = get_cached_block(bridge, block_cache, block_hash, false).await?;
        sortable.push((block_hash, block.header.blue_work, block.header.timestamp));
    }
    sortable.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    merged_blocks.extend(sortable.into_iter().map(|(hash, _blue_work, timestamp)| {
        ReconstructedMergedBlockAcceptance {
            hash,
            timestamp,
            transactions: Vec::new(),
        }
    }));

    let mut block_indexes = AHashMap::<RpcHash, usize>::new();
    for (index, block) in merged_blocks.iter().enumerate() {
        block_indexes.insert(block.hash, index);
    }

    for rpc_tx in &accepted.accepted_transactions {
        let Some(verbose_data) = rpc_tx.verbose_data.as_ref() else {
            return Err(Error::custom(format!(
                "accepted transaction in chain block {accepted_chain_block_hash} is missing verbose data"
            )));
        };
        let Some(block_hash) = verbose_data.block_hash else {
            return Err(Error::custom(format!(
                "accepted transaction in chain block {accepted_chain_block_hash} is missing verbose block hash"
            )));
        };
        let Some(index) = block_indexes.get(&block_hash).copied() else {
            return Err(Error::custom(format!(
                "accepted transaction from block {block_hash} was not found in reconstructed mergeset for {accepted_chain_block_hash}"
            )));
        };
        merged_blocks[index].transactions.push(rpc_tx.clone());
    }

    Ok(ReconstructedChainBlockAcceptance {
        accepted_chain_block_hash,
        accepting_block_blue_score,
        accepting_block_daa_score,
        merged_blocks,
    })
}

async fn get_cached_block(
    bridge: &Arc<dyn BridgeT>,
    block_cache: &mut AHashMap<RpcHash, RpcBlock>,
    hash: RpcHash,
    include_transactions: bool,
) -> Result<RpcBlock> {
    if let Some(block) = block_cache.get(&hash) {
        return Ok(block.clone());
    }

    let block = bridge.get_block(hash, include_transactions).await?;
    block_cache.insert(hash, block.clone());
    Ok(block)
}

fn is_origin_hash(hash: &RpcHash) -> bool {
    hash.as_bytes().iter().all(|byte| *byte == ORIGIN_HASH_BYTE)
}

fn process_reconstructed_acceptance_data(
    chain_block_accepted_transactions: &[ReconstructedChainBlockAcceptance],
    analyzer: &Analyzer,
) -> Vec<Mergeset> {
    let mut collected_mergesets = Vec::new();

    for accepted in chain_block_accepted_transactions {
        let mut entropy_builder = MergesetEntropyBuilder::default();
        let accepting_block_blue_score = accepted.accepting_block_blue_score;
        let accepting_block_daa_score = accepted.accepting_block_daa_score;
        let accepted_chain_block_hash = accepted.accepted_chain_block_hash;

        for merged_block in &accepted.merged_blocks {
            entropy_builder.add_block_hash(&merged_block.hash);
        }

        let operations = accepted
            .merged_blocks
            .iter()
            .enumerate()
            .flat_map(|(block_index_within_mergeset, merged_block)| {
                merged_block
                    .transactions
                    .iter()
                    .enumerate()
                    .map(move |(index_within_merged_block, rpc_tx)| {
                        let fee = rpc_transaction_fee(rpc_tx);
                        let tx = Transaction::try_from(rpc_tx.clone())
                            .inspect_err(|err| error!("failed to convert rpcTx to tx with err: {err}"))
                            .ok()?;
                        let ctx_tx = ContextTransaction {
                            tx,
                            fee,
                            block_time: merged_block.timestamp,
                            accepting_block_daa_score,
                            index_within_merged_block,
                        };

                        analyzer
                            .detect_krc721(&ctx_tx)
                            .map_err(|err| (ctx_tx.tx.id(), err))
                            .inspect_err(|(txid, err)| {
                                error!("{txid} - detect krc721 error: {err}");
                                if let Some(db) = analyzer.db() {
                                    let txid = *txid;
                                    let reason = err.to_string();
                                    let db = db.clone();
                                    spawn_blocking(move || {
                                        let mut wtx = db.write_tx();
                                        _ = db.reject_tx(&mut wtx, txid, &reason).inspect_err(|err| {
                                            error!("failed to store transaction rejection in db: {err}")
                                        });
                                        let _ = wtx.commit().inspect_err(|err| {
                                            error!("failed to commit rejected transaction wtx in db: {err}")
                                        });
                                    });
                                }
                            })
                            .ok()
                            .flatten()
                            .map(|operation| MergesetOperation {
                                block_index_within_mergeset,
                                operation,
                                index_within_merged_block,
                            })
                    })
            })
            .flatten()
            .collect();

        collected_mergesets.push(Mergeset {
            operations,
            entropy: entropy_builder.finalize(),
            blue_score: accepting_block_blue_score,
            accepted_chain_block_hash,
        });
    }

    collected_mergesets
}

struct ReconstructedChainBlockAcceptance {
    accepted_chain_block_hash: RpcHash,
    accepting_block_blue_score: u64,
    accepting_block_daa_score: u64,
    merged_blocks: Vec<ReconstructedMergedBlockAcceptance>,
}

impl ReconstructedChainBlockAcceptance {
    fn from_flat_accepted_transactions(accepted: &RpcChainBlockAcceptedTransactions) -> Self {
        let accepting_block_blue_score = accepted.chain_block_header.blue_score.unwrap_or_default();
        let accepting_block_daa_score = accepted.chain_block_header.daa_score.unwrap_or_default();
        let accepted_chain_block_hash = accepted.chain_block_header.hash.unwrap_or_default();
        let mut merged_blocks = Vec::<ReconstructedMergedBlockAcceptance>::new();
        let mut merged_block_indexes = AHashMap::<RpcHash, usize>::new();

        for rpc_tx in &accepted.accepted_transactions {
            let Some(verbose_data) = rpc_tx.verbose_data.as_ref() else {
                warn!("skipping accepted transaction without verbose data");
                continue;
            };
            let Some(merged_block_hash) = verbose_data.block_hash else {
                warn!("skipping accepted transaction without verbose block hash");
                continue;
            };
            let Some(block_time) = verbose_data.block_time else {
                warn!("skipping accepted transaction without verbose block time");
                continue;
            };
            let index = *merged_block_indexes
                .entry(merged_block_hash)
                .or_insert_with(|| {
                    let index = merged_blocks.len();
                    merged_blocks.push(ReconstructedMergedBlockAcceptance {
                        hash: merged_block_hash,
                        timestamp: block_time,
                        transactions: Vec::new(),
                    });
                    index
                });
            merged_blocks[index].transactions.push(rpc_tx.clone());
        }

        Self {
            accepted_chain_block_hash,
            accepting_block_blue_score,
            accepting_block_daa_score,
            merged_blocks,
        }
    }
}

struct ReconstructedMergedBlockAcceptance {
    hash: RpcHash,
    timestamp: u64,
    transactions: Vec<RpcOptionalTransaction>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_rpc_core::{RpcOptionalHeader, RpcOptionalTransactionVerboseData};

    fn entropy(hashes: &[RpcHash]) -> u64 {
        let mut builder = MergesetEntropyBuilder::default();
        for hash in hashes {
            builder.add_block_hash(hash);
        }
        builder.finalize()
    }

    fn optional_transaction(
        verbose_data: Option<RpcOptionalTransactionVerboseData>,
    ) -> RpcOptionalTransaction {
        RpcOptionalTransaction {
            version: None,
            inputs: vec![],
            outputs: vec![],
            lock_time: None,
            subnetwork_id: None,
            gas: None,
            payload: None,
            storage_mass: None,
            verbose_data,
        }
    }

    fn acceptance_header(hash: Option<RpcHash>) -> RpcChainBlockAcceptedTransactions {
        RpcChainBlockAcceptedTransactions {
            chain_block_header: RpcOptionalHeader {
                hash,
                blue_score: Some(42),
                daa_score: Some(24),
                ..RpcOptionalHeader::default()
            },
            accepted_transactions: vec![],
        }
    }

    #[test]
    fn historical_acceptance_coverage_requires_one_record_per_added_block() {
        let first = RpcHash::from_le_u64([0x11, 0x12, 0x13, 0x14]);
        let second = RpcHash::from_le_u64([0x21, 0x22, 0x23, 0x24]);

        let err = validate_historical_acceptance_coverage(
            &[first, second],
            &[acceptance_header(Some(first))],
        )
        .unwrap_err();

        assert!(err.to_string().contains("does not match"));
    }

    #[test]
    fn historical_acceptance_coverage_rejects_missing_or_wrong_hashes() {
        let expected = RpcHash::from_le_u64([0x11, 0x12, 0x13, 0x14]);
        let unexpected = RpcHash::from_le_u64([0x21, 0x22, 0x23, 0x24]);

        let missing_err =
            validate_historical_acceptance_coverage(&[expected], &[acceptance_header(None)])
                .unwrap_err();
        assert!(missing_err
            .to_string()
            .contains("missing accepted chain block hash"));

        let mismatch_err = validate_historical_acceptance_coverage(
            &[expected],
            &[acceptance_header(Some(unexpected))],
        )
        .unwrap_err();
        assert!(mismatch_err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn historical_acceptance_coverage_accepts_matching_ordered_records() {
        let first = RpcHash::from_le_u64([0x11, 0x12, 0x13, 0x14]);
        let second = RpcHash::from_le_u64([0x21, 0x22, 0x23, 0x24]);

        validate_historical_acceptance_coverage(
            &[first, second],
            &[
                acceptance_header(Some(first)),
                acceptance_header(Some(second)),
            ],
        )
        .unwrap();
    }

    #[test]
    fn sync_task_claim_allows_only_one_runner() {
        let running = AtomicBool::new(false);

        assert!(try_claim_sync_task_flag(&running));
        assert!(!try_claim_sync_task_flag(&running));

        running.store(false, Ordering::SeqCst);
        assert!(try_claim_sync_task_flag(&running));
    }

    #[test]
    fn sync_point_requires_same_block_hash_not_just_score_ordering() {
        let old_tip = BlueScoredChainBlockHash {
            blue_score: 100,
            block_hash: RpcHash::from_le_u64([0x11, 0x12, 0x13, 0x14]),
        };
        let new_sink_same_score = BlueScoredChainBlockHash {
            blue_score: 100,
            block_hash: RpcHash::from_le_u64([0x21, 0x22, 0x23, 0x24]),
        };
        let new_sink_lower_score = BlueScoredChainBlockHash {
            blue_score: 99,
            block_hash: RpcHash::from_le_u64([0x31, 0x32, 0x33, 0x34]),
        };

        assert!(old_tip >= new_sink_lower_score);
        assert!(!is_same_sync_point(old_tip, new_sink_same_score));
        assert!(!is_same_sync_point(old_tip, new_sink_lower_score));
        assert!(is_same_sync_point(old_tip, old_tip));
    }

    #[test]
    fn classifies_target_progress_by_hash_and_replay_tip() {
        let target = BlueScoredChainBlockHash {
            blue_score: 100,
            block_hash: RpcHash::from_le_u64([0x11, 0x12, 0x13, 0x14]),
        };
        let below = BlueScoredChainBlockHash {
            blue_score: 99,
            block_hash: RpcHash::from_le_u64([0x21, 0x22, 0x23, 0x24]),
        };
        let same_score_different_hash = BlueScoredChainBlockHash {
            blue_score: 100,
            block_hash: RpcHash::from_le_u64([0x31, 0x32, 0x33, 0x34]),
        };
        let later = BlueScoredChainBlockHash {
            blue_score: 101,
            block_hash: RpcHash::from_le_u64([0x41, 0x42, 0x43, 0x44]),
        };

        assert_eq!(
            classify_target_progress(&[], None, target),
            TargetProgress::Pending
        );
        assert_eq!(
            classify_target_progress(&[target.block_hash], Some(later), target),
            TargetProgress::Reached
        );
        assert_eq!(
            classify_target_progress(&[below.block_hash], Some(below), target),
            TargetProgress::Pending
        );
        assert_eq!(
            classify_target_progress(
                &[same_score_different_hash.block_hash],
                Some(same_score_different_hash),
                target
            ),
            TargetProgress::Obsolete
        );
        assert_eq!(
            classify_target_progress(&[later.block_hash], Some(later), target),
            TargetProgress::Obsolete
        );

        let prod2_stale_target = BlueScoredChainBlockHash {
            blue_score: 452_346_424,
            block_hash: RpcHash::from_le_u64([0x51, 0x52, 0x53, 0x54]),
        };
        let prod2_replay_tip = BlueScoredChainBlockHash {
            blue_score: 452_348_469,
            block_hash: RpcHash::from_le_u64([0x61, 0x62, 0x63, 0x64]),
        };

        assert_eq!(
            classify_target_progress(
                &[prod2_replay_tip.block_hash],
                Some(prod2_replay_tip),
                prod2_stale_target
            ),
            TargetProgress::Obsolete
        );
    }

    #[test]
    fn detects_kaspa_origin_hash() {
        let origin = RpcHash::from_bytes([ORIGIN_HASH_BYTE; 32]);
        let non_origin = RpcHash::from_le_u64([0x11, 0x12, 0x13, 0x14]);

        assert!(is_origin_hash(&origin));
        assert!(!is_origin_hash(&non_origin));
    }

    #[test]
    fn reconstructed_acceptance_preserves_empty_merged_blocks_in_entropy() {
        let selected_parent = RpcHash::from_le_u64([0x11, 0x12, 0x13, 0x14]);
        let empty_merged = RpcHash::from_le_u64([0x21, 0x22, 0x24, 0x28]);
        let accepted_chain = RpcHash::from_le_u64([0x31, 0x32, 0x33, 0x34]);
        let acceptance = ReconstructedChainBlockAcceptance {
            accepted_chain_block_hash: accepted_chain,
            accepting_block_blue_score: 42,
            accepting_block_daa_score: 24,
            merged_blocks: vec![
                ReconstructedMergedBlockAcceptance {
                    hash: selected_parent,
                    timestamp: 1,
                    transactions: vec![],
                },
                ReconstructedMergedBlockAcceptance {
                    hash: empty_merged,
                    timestamp: 2,
                    transactions: vec![],
                },
            ],
        };
        let analyzer = Analyzer::new(None, Default::default(), Prefix::Mainnet, Arc::new([]), 0);

        let mergesets = process_reconstructed_acceptance_data(&[acceptance], &analyzer);

        assert_eq!(mergesets.len(), 1);
        assert_eq!(mergesets[0].accepted_chain_block_hash, accepted_chain);
        assert_eq!(mergesets[0].blue_score, 42);
        assert_eq!(mergesets[0].operations.len(), 0);
        assert_eq!(
            mergesets[0].entropy,
            entropy(&[selected_parent, empty_merged])
        );
        assert_ne!(mergesets[0].entropy, entropy(&[selected_parent]));
    }

    #[test]
    fn flat_reconstruction_skips_transactions_missing_verbose_block_fields() {
        let accepted_chain = RpcHash::from_le_u64([0x31, 0x32, 0x33, 0x34]);
        let merged_block = RpcHash::from_le_u64([0x41, 0x42, 0x43, 0x44]);
        let accepted = RpcChainBlockAcceptedTransactions {
            chain_block_header: RpcOptionalHeader {
                hash: Some(accepted_chain),
                blue_score: Some(42),
                daa_score: Some(24),
                ..RpcOptionalHeader::default()
            },
            accepted_transactions: vec![
                optional_transaction(None),
                optional_transaction(Some(RpcOptionalTransactionVerboseData {
                    transaction_id: None,
                    hash: None,
                    compute_mass: None,
                    block_hash: None,
                    block_time: Some(100),
                })),
                optional_transaction(Some(RpcOptionalTransactionVerboseData {
                    transaction_id: None,
                    hash: None,
                    compute_mass: None,
                    block_hash: Some(merged_block),
                    block_time: None,
                })),
                optional_transaction(Some(RpcOptionalTransactionVerboseData {
                    transaction_id: None,
                    hash: None,
                    compute_mass: None,
                    block_hash: Some(merged_block),
                    block_time: Some(100),
                })),
            ],
        };

        let reconstructed =
            ReconstructedChainBlockAcceptance::from_flat_accepted_transactions(&accepted);

        assert_eq!(reconstructed.accepted_chain_block_hash, accepted_chain);
        assert_eq!(reconstructed.accepting_block_blue_score, 42);
        assert_eq!(reconstructed.accepting_block_daa_score, 24);
        assert_eq!(reconstructed.merged_blocks.len(), 1);
        assert_eq!(reconstructed.merged_blocks[0].hash, merged_block);
        assert_eq!(reconstructed.merged_blocks[0].timestamp, 100);
        assert_eq!(reconstructed.merged_blocks[0].transactions.len(), 1);
    }
}

fn rpc_transaction_fee(tx: &RpcOptionalTransaction) -> u64 {
    let input_sum = tx
        .inputs
        .iter()
        .filter_map(|input| input.verbose_data.as_ref())
        .filter_map(|verbose| verbose.utxo_entry.as_ref())
        .filter_map(|utxo| utxo.amount)
        .sum::<u64>();
    let output_sum = tx
        .outputs
        .iter()
        .filter_map(|output| output.value)
        .sum::<u64>();
    input_sum.saturating_sub(output_sum)
}

#[derive(Default)]
pub struct MergesetEntropyBuilder {
    entropy: u64,
}

impl MergesetEntropyBuilder {
    pub fn add_block_hash(&mut self, hash: &RpcHash) {
        // Process all 32 bytes of the hash in 8-byte chunks
        self.entropy = hash.as_bytes().chunks(8).fold(self.entropy, |accum, item| {
            accum ^ u64::from_le_bytes(item.try_into().unwrap())
        });
    }

    pub fn finalize(self) -> u64 {
        self.entropy
    }
}

pub trait SyncerT: Send + Sync + 'static {
    fn is_synced(&self) -> bool;
    fn last_known_block(&self) -> Option<BlueScoredChainBlockHash>;
    fn spawn(self: Arc<Self>, last_known_block: BlueScoredChainBlockHash);
    fn shutdown(&self);
}
