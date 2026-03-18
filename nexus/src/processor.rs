use crate::metrics::Counters;
use crate::{calculate_blue_score_from_tx_score, calculate_tx_score, calculate_tx_score_from_blue};
use ahash::AHashMap;
use async_std::channel::Sender;
use async_trait::async_trait;
use crossbeam_channel::SendError;
pub use error::Error;
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_rpc_core::RpcHash;
use krc721_core::model::krc721::{
    BlueScoredChainBlockHash, CheckedOperation, CtxValidationError, DeployInfo,
    DeployInfoWithCommon, DiscountInfo, ListingInfo, Mergeset, MintInfo, Operation,
    OperationCommon, OperationInfo, ScoredCheckedOperation, SendInfo, Tick, TransferInfo,
    VirtualChainChanges,
};
use krc721_core::runtime::{Runtime, Service, ServiceError, ServiceResult};
use krc721_database::database::{
    AddressHoldingKey, CurrentOwnershipValue, Db, DeploymentKey, ListingByTickKey, ListingValue,
    MintHistoryKey, OwnershipHistoryKey, OwnershipKey, ScoredDeployInfoWithCommon,
    ScoredDiscountKey, StatsDiffs, TokenMintsKey, VipKey, WriteTransaction,
};
pub use result::Result;
use std::num::NonZeroU64;
use std::{
    sync::{Arc, Mutex},
    thread,
};
use thiserror::Error;
use tokio::task::spawn_blocking;
use tracing::{debug, debug_span, error, info, instrument, warn};

mod error;
mod result;
mod token_id;
pub enum RTNotification {
    SwitchToQueue,
    VirtualChainChangesNotification(VirtualChainChanges),
    ApplyQueue(crossbeam_channel::Sender<()>),
}

pub enum HTNotification {
    HistoricalVirtualChainChangesNotification(VirtualChainChanges),
    ApplicationHistoricalVirtualChainChangesNotification(VirtualChainChanges),
}

type RealTimeThreadHandle = thread::JoinHandle<()>;
type HistoricalThreadHandle = thread::JoinHandle<()>;

pub struct Processor {
    realtime_sender: crossbeam_channel::Sender<RTNotification>,
    realtime_receiver: crossbeam_channel::Receiver<RTNotification>,

    shutdown_sender: crossbeam_channel::Sender<()>,
    shutdown_receiver: crossbeam_channel::Receiver<()>,

    historical_sender: crossbeam_channel::Sender<HTNotification>,
    historical_receiver: crossbeam_channel::Receiver<HTNotification>,

    historical_switch_sender: crossbeam_channel::Sender<()>,
    historical_switch_receiver: crossbeam_channel::Receiver<()>,

    ingest_thread_handle: Mutex<Option<(RealTimeThreadHandle, HistoricalThreadHandle)>>,
    db: Arc<Db>,
    counters: Arc<Counters>,
    checked_op_sender: Option<Sender<ScoredCheckedOperation>>,
    tx_write_sender: Option<Sender<()>>,
    // TODO - ADD METRICS HERE
}

impl Processor {
    pub fn new(
        db: Arc<Db>,
        counters: Arc<Counters>,
        sender: Option<Sender<ScoredCheckedOperation>>,
        tx_write_sender: Option<Sender<()>>,
    ) -> Self {
        let (shutdown_sender, shutdown_receiver) = crossbeam_channel::bounded(1);
        let (rtvccn_sender, rtvccn_receiver) = crossbeam_channel::unbounded();
        let (historical_sender, historical_receiver) = crossbeam_channel::unbounded();
        let (historical_mode_sender, historical_mode_receiver) = crossbeam_channel::bounded(1);

        Self {
            realtime_sender: rtvccn_sender,
            realtime_receiver: rtvccn_receiver,
            shutdown_sender,
            shutdown_receiver,
            historical_sender,
            historical_receiver,
            historical_switch_sender: historical_mode_sender,
            historical_switch_receiver: historical_mode_receiver,
            ingest_thread_handle: Default::default(),
            db,
            counters,
            checked_op_sender: sender, // Add this field when testing feature is enabled
            tx_write_sender,
        }
    }

    pub fn send_realtime_virtual_chain_changed_notification(
        &self,
        notification: VirtualChainChanges,
    ) -> Result<(), SendError<RTNotification>> {
        self.realtime_sender
            .send(RTNotification::VirtualChainChangesNotification(
                notification,
            ))
    }

    pub fn send_historical_virtual_chain_changed_notification(
        &self,
        notification: VirtualChainChanges,
    ) -> Result<(), SendError<HTNotification>> {
        self.historical_sender
            .send(HTNotification::HistoricalVirtualChainChangesNotification(
                notification,
            ))
    }

    pub fn send_historical_virtual_chain_changed_notification_and_apply_queue(
        &self,
        notification: VirtualChainChanges,
    ) -> Result<(), SendError<HTNotification>> {
        self.historical_sender.send(
            HTNotification::ApplicationHistoricalVirtualChainChangesNotification(notification),
        )
    }

    pub fn switch_to_queue_mod(&self) -> Result<(), SendError<RTNotification>> {
        self.realtime_sender.send(RTNotification::SwitchToQueue)
    }

    #[instrument(skip(self))]
    pub fn htvccn_task(&self) -> Result<()> {
        info!("htvccn_task started");
        let mut id = 0u64;
        let mut wait_for_switch = false;
        loop {
            if wait_for_switch {
                crossbeam_channel::select_biased!(
                    recv(self.shutdown_receiver) -> _ => {
                        info!("shutdown received");
                        return Ok(());
                    }
                    recv(self.historical_switch_receiver) -> _ => {
                        debug!("switch received, stop waiting");
                        wait_for_switch = false;
                    },
                )
            }
            id += 1;
            let _span = debug_span!("htvccn", id = id).entered();
            let ingest_event = crossbeam_channel::select_biased!(
                recv(self.shutdown_receiver) -> _ => {
                    info!("shutdown received");
                    return Ok(());
                }
                recv(self.historical_receiver) -> msg => msg,
            );
            {
                match ingest_event.inspect_err(|err| error!("error receiving message: {err}"))? {
                    HTNotification::HistoricalVirtualChainChangesNotification(vcc) => {
                        debug!("historical virtual chain changes notification received");

                        self.process_chain_changes(vcc)?
                    }
                    HTNotification::ApplicationHistoricalVirtualChainChangesNotification(vcc) => {
                        debug!(
                            "application historical virtual chain changes notification received"
                        );
                        self.process_chain_changes(vcc)?;
                        let (sender, receiver) = crossbeam_channel::bounded(0);
                        debug!("applying queue, processing changes is done");
                        self.realtime_sender
                            .send(RTNotification::ApplyQueue(sender))
                            .map_err(|_| Error::SendError)?;
                        _ = receiver.recv().inspect_err(|err| {
                            error!("application queue didn't response, err: {err}")
                        });
                        wait_for_switch = true;
                    }
                }
            }
        }
    }

    #[instrument(skip(self))]
    pub fn rtvccn_task(&self) -> Result<()> {
        info!("rtvccn_task started");
        let mut id = 0u64;
        let mut real_time_mod_enabled = false;
        loop {
            id += 1;
            let _span = debug_span!("rtvccn", id = id).entered();
            let ingest_event = crossbeam_channel::select_biased!(
                recv(self.shutdown_receiver) -> _ => {
                    info!("shutdown received");
                    return Ok(())
                }
                recv(self.realtime_receiver) -> msg => msg,
            );
            match ingest_event.inspect_err(|err| error!("error receiving message: {err}"))? {
                RTNotification::ApplyQueue(sender) => {
                    debug!("apply queue notification received");
                    let res = self.process_queue_application();
                    _ = sender.send(()).inspect_err(|err| {
                        error!("failed to send response after queue application: {err}")
                    });
                    real_time_mod_enabled = true;
                    res?
                }
                RTNotification::SwitchToQueue => {
                    real_time_mod_enabled = false;
                    _ = self.historical_switch_sender.send(()).inspect_err(|err| {
                        error!("failed to send `QueueSwitched` to historical channel: {err}")
                    });
                }
                RTNotification::VirtualChainChangesNotification(vcc) if real_time_mod_enabled => {
                    debug!("realtime virtual chain changes notification received");
                    self.process_chain_changes(vcc)
                        .inspect_err(|err| error!("error processing chain changes: {err}"))?
                }
                RTNotification::VirtualChainChangesNotification(vcc) => {
                    debug!("queue virtual chain changes notification received");
                    self.process_queue(vcc)?
                }
            }
        }
    }

    /// Process realtime chain changes by handling block reorganizations and new block additions.
    /// This is the main entry point for processing virtual chain changes.
    fn process_chain_changes(&self, vcc: VirtualChainChanges) -> Result<()> {
        let mut tx = self.db.write_tx();
        self.process_chain_changes_wtx(vcc, &mut tx)?;

        tx.commit()
            .map_err(krc721_database::error::Error::Fjall)? // todo should be abstract error rather than fjall
            .expect("no conflict should happen");

        if let Some(sender) = self.tx_write_sender.as_ref() {
            sender.try_send(()).expect("send should never fail");
        }

        Ok(())
    }

    fn process_chain_changes_wtx(
        &self,
        VirtualChainChanges {
            removed_chain_block_hashes,
            mergesets,
        }: VirtualChainChanges,
        tx: &mut WriteTransaction,
    ) -> Result<()> {
        // Phase 1: Process chain reorganization by removing invalidated blocks and their associated data
        if !removed_chain_block_hashes.is_empty() {
            let stats_diffs = self.process_removal(tx, &removed_chain_block_hashes)?;
            let stats = self.db.stats.removal(tx, stats_diffs)?;
            self.counters.update_from_stats(&stats);
        }

        // Phase 2: Process new blocks by validating and applying their operations in chronological order
        if !mergesets.is_empty() {
            let stats_diffs = self.process_additions(tx, mergesets)?;
            let stats = self.db.stats.addition(tx, stats_diffs)?;
            self.counters.update_from_stats(&stats);
        }
        Ok(())
    }

    /// Process chain reorganization by removing blocks and their operations, then reconstructing the state
    /// from the remaining valid history.
    ///
    /// This function handles the complex process of removing invalidated blocks during a reorg:
    /// 1. Removes affected blocks and finds minimum blue score
    /// 2. Deletes operations above the score threshold
    /// 3. Removes affected deployments
    /// 4. Rebuilds token ownership state from valid history
    fn process_removal(
        &self,
        tx: &mut WriteTransaction,
        removed_blocks: &[RpcHash],
    ) -> Result<StatsDiffs> {
        let mut stat_diffs = StatsDiffs::default();
        // Step 1: Calculate minimum blue score of affected blocks and remove them from chain state
        let min_blue_score = self.identify_reorg_blocks_and_delete(tx, removed_blocks)?;
        let Some(min_blue_score) = min_blue_score else {
            warn!("No blocks to delete in reorg according to db state");
            return Ok(stat_diffs);
        };

        // Step 2: Calculate transaction score threshold for dependent data cleanup
        let tx_score_threshold = calculate_tx_score_from_blue(min_blue_score);

        // Step 3: Remove NFT operations above the score threshold to maintain consistency
        stat_diffs = self.remove_affected_operations(tx, tx_score_threshold)?;

        // Step 4: Remove collection deployments invalidated by the reorganization
        let premint_from_removed = self.remove_affected_deployments(tx, tx_score_threshold)?;

        self.remove_discounts(tx, tx_score_threshold)?;

        // Step 5: Reconstruct token ownership state from remaining valid history
        self.reconstruct_token_ownership(tx, tx_score_threshold, premint_from_removed)?;

        Ok(stat_diffs)
    }

    /// Phase 2: Process new blocks and their operations
    #[instrument(skip_all)]
    fn process_additions(
        &self,
        tx: &mut WriteTransaction,
        mergesets: Vec<Mergeset>,
    ) -> Result<StatsDiffs> {
        let mut stats_diff = StatsDiffs::default();
        let last_accepted_blue_score = self
            .db
            .chain_block_scores
            .last_accepted_block_wtx(tx)?
            .inspect(|bb| debug!("last block is: {bb:?}"))
            .map(|BlueScoredChainBlockHash { blue_score, .. }| blue_score)
            .unwrap_or_default();
        for mergeset in mergesets {
            if mergeset.blue_score < last_accepted_blue_score {
                warn!(
                    "Block {} has blue score {} lower than last accepted block {}. Ignoring.",
                    mergeset.accepted_chain_block_hash,
                    mergeset.blue_score,
                    last_accepted_blue_score
                );
                continue;
            }
            // Step 1: Add new blocks to chain state
            let inserted = self.db.chain_block_scores.insert_if_not_exist_wtx(
                tx,
                BlueScoredChainBlockHash {
                    blue_score: mergeset.blue_score,
                    block_hash: mergeset.accepted_chain_block_hash,
                },
                &(),
            )?;
            debug!(
                "Block {} with score is inserted: {}",
                mergeset.accepted_chain_block_hash, mergeset.blue_score
            );
            if !inserted {
                warn!(
                    "Block {} already exists in chain state. Ignoring.",
                    mergeset.accepted_chain_block_hash
                );
                continue;
            }

            self.db.blockhash_to_score.insert_wtx(
                tx,
                mergeset.accepted_chain_block_hash,
                &mergeset.blue_score,
            )?;
            // Step 2: Process NFT operations in order by blue_score
            let scored_operations = mergeset
                .operations
                .into_iter()
                .filter_map(|op| {
                    let tx_score = calculate_tx_score(
                        mergeset.blue_score,
                        op.block_index_within_mergeset as u64,
                        op.index_within_merged_block as u64,
                    );
                    if let Some(opscore) = self
                        .db
                        .tx_id_to_opscore
                        .get_wtx(tx, &op.operation.common.tx_id)
                        .ok()?
                    {
                        let block_blue_score = calculate_blue_score_from_tx_score(opscore);
                        warn!(
                            "Operation with tx_id {} already exists in chain state. Ignoring.",
                            op.operation.common.tx_id
                        );
                        let block = self
                            .db
                            .chain_block_scores
                            .range_wtx(
                                tx,
                                &BlueScoredChainBlockHash {
                                    blue_score: block_blue_score,
                                    block_hash: Default::default(),
                                }..&BlueScoredChainBlockHash {
                                    blue_score: block_blue_score + 1,
                                    block_hash: Default::default(),
                                },
                            )
                            .next()?
                            .ok()?
                            .0
                            .block_hash;
                        warn!(
                            "Operation with tx_id {} was in block: {}; current block is {}",
                            op.operation.common.tx_id, block, mergeset.accepted_chain_block_hash
                        );
                        None
                    } else {
                        Some(ContextOperation {
                            tx_score,
                            operation: op.operation,
                            mergeset_entropy: mergeset.entropy,
                        })
                    }
                })
                .collect::<Vec<_>>();
            stats_diff += self.process_nft_operations(tx, scored_operations)?;
        }
        Ok(stats_diff)
    }

    /// Process NFT operations in chronological order
    fn process_nft_operations(
        &self,
        wtx: &mut WriteTransaction,
        operations: impl IntoIterator<Item = ContextOperation>,
    ) -> Result<StatsDiffs> {
        let mut diff = StatsDiffs::default();
        for ContextOperation {
            tx_score,
            mut operation,
            mergeset_entropy,
        } in operations
        {
            let validation_err = match &mut operation.info {
                OperationInfo::Deploy(deploy_info) => self
                    .process_deployment(wtx, tx_score, &operation.common, deploy_info)?
                    .inspect(|_| {
                        diff.mints += deploy_info.premint;
                        diff.security_fees += operation.common.fee;
                        diff.deployments += 1;
                        diff.royalty_fees += deploy_info
                            .royalty
                            .as_ref()
                            .map(|rd| rd.fee)
                            .unwrap_or_default();
                    })
                    .err(),
                OperationInfo::Mint(mint_info) => self
                    .process_mint(
                        wtx,
                        mergeset_entropy,
                        tx_score,
                        &operation.common,
                        mint_info,
                        false,
                    )?
                    .inspect(|_| {
                        diff.security_fees += operation.common.fee;
                        diff.mints += 1;
                        diff.royalty_fees += mint_info
                            .royalty
                            .as_ref()
                            .map(|rd| rd.fee)
                            .unwrap_or_default();
                    })
                    .err(),
                OperationInfo::Transfer(transfer_info) => self
                    .process_transfer(wtx, tx_score, &operation.common, transfer_info)?
                    .inspect(|_| {
                        diff.security_fees += operation.common.fee;
                        diff.transfers += 1;
                    })
                    .err(),
                OperationInfo::Discount(discount_info) => self
                    .process_discount(wtx, tx_score, &operation.common, discount_info)?
                    .inspect(|_| {
                        diff.security_fees += operation.common.fee;
                        // todo should we calculate discounts?
                    })
                    .err(),
                OperationInfo::List(list_info) => self
                    .process_list(wtx, tx_score, &operation.common, list_info)?
                    .inspect(|_| {
                        diff.security_fees += operation.common.fee;
                        diff.listings += 1;
                    })
                    .err(),
                OperationInfo::Send(send_info) => self
                    .process_send(wtx, tx_score, &operation.common, send_info)?
                    .inspect(|_| {
                        diff.security_fees += operation.common.fee;
                        diff.sends += 1;
                    })
                    .err(),
            };

            let checked_operation = CheckedOperation {
                operation,
                error: validation_err,
            };
            // Record the processed operation
            self.db
                .operation_history
                .insert_wtx(wtx, tx_score, &checked_operation)?;
            self.db.tx_id_to_opscore.insert_wtx(
                wtx,
                checked_operation.operation.common.tx_id,
                &tx_score,
            )?;

            // #[cfg(feature = "testing")]
            if let Some(sender) = self.checked_op_sender.as_ref() {
                if let Err(err) = sender.try_send(ScoredCheckedOperation {
                    opscore: tx_score,
                    checked_operation,
                }) {
                    tracing::error!("Failed to send checked operation to test channel: {}", err);
                }
            }
        }
        Ok(diff)
    }

    /// Remove blocks affected by reorg and determine the minimum score threshold.
    /// Returns the minimum blue score of removed blocks, or None if no blocks were found.
    #[instrument(skip_all)]
    fn identify_reorg_blocks_and_delete(
        &self,
        tx: &mut WriteTransaction,
        removed_blocks: &[RpcHash],
    ) -> Result<Option<u64>> {
        let mut min_blue_score = u64::MAX;
        for block in removed_blocks {
            let Some(score) = self.db.blockhash_to_score.get_wtx(tx, block)? else {
                warn!("Block {} not found in blockhash_to_score. Ignoring.", block);
                continue;
            };
            min_blue_score = min_blue_score.min(score);
            self.db.chain_block_scores.remove_wtx(
                tx,
                &BlueScoredChainBlockHash {
                    blue_score: score,
                    block_hash: *block,
                },
            )?;
            self.db.blockhash_to_score.remove_wtx(tx, block)?;
        }
        debug!("Reorg complete minimum blue score {}", min_blue_score);

        Ok(Some(min_blue_score))
    }

    /// Remove NFT operations above the given score threshold to maintain consistency after reorg.
    fn remove_affected_operations(
        &self,
        tx: &mut WriteTransaction,
        score_threshold: u64,
    ) -> krc721_database::result::Result<StatsDiffs> {
        let ops_to_remove = self
            .db
            .operation_history
            .range_wtx(tx, score_threshold..)
            .collect::<krc721_database::result::Result<Vec<_>>>()?;
        if !ops_to_remove.is_empty() {
            warn!("Ops to remove: {:?}", ops_to_remove);
        }
        let mut diff = StatsDiffs::default();
        for (score, op) in ops_to_remove {
            self.db
                .tx_id_to_opscore
                .remove_wtx(tx, &op.operation.common.tx_id)?;

            let CheckedOperation {
                operation,
                error: Option::None,
            } = self
                .db
                .operation_history
                .remove_if_exists_wtx(tx, &score)?
                .unwrap()
            else {
                continue;
            };
            diff.security_fees += operation.common.fee; // todo should we calculate security fee for transfer??
            match operation.info {
                OperationInfo::Deploy(d) => {
                    diff.deployments += 1;
                    diff.royalty_fees += d.royalty.map(|rd| rd.fee).unwrap_or_default();
                }
                OperationInfo::Mint(m) => {
                    diff.mints += 1;
                    diff.royalty_fees += m.royalty.map(|rd| rd.fee).unwrap_or_default();
                }
                OperationInfo::Transfer(_) => {
                    diff.transfers += 1;
                }
                OperationInfo::Discount(_) => {
                    // do nothing
                }
                OperationInfo::List(_) => {
                    diff.listings += 1;
                }
                OperationInfo::Send(_) => {
                    diff.sends += 1;
                }
            }
        }
        Ok(diff)
    }

    /// Remove collection deployments above the score threshold to maintain consistency after reorg.
    fn remove_affected_deployments(
        &self,
        tx: &mut WriteTransaction,
        score_threshold: u64,
    ) -> krc721_database::result::Result<AHashMap<Tick, u64>> {
        let deployments_to_delete = self
            .db
            .collection_deployments
            .range_keys_wtx(
                tx,
                DeploymentKey {
                    score: score_threshold,
                    tick: Tick::MIN,
                }..,
            )
            .collect::<krc721_database::result::Result<Vec<_>>>()?;
        let mut premint_count: AHashMap<Tick, u64> = AHashMap::new();
        for key in deployments_to_delete {
            self.db.collection_deployments.remove_wtx(tx, &key)?;
            if let Some(deployment) = self
                .db
                .collection_registry
                .remove_if_exists_wtx(tx, &key.tick)?
            {
                premint_count.insert(key.tick, deployment.info.info.premint);
            }
        }
        Ok(premint_count)
    }
    /// Remove and reconstruct token ownership state for affected tokens.
    /// This involves:
    /// 1. Getting affected token operations
    /// 2. Removing current ownership records
    /// 3. Rebuilding ownership state from valid history
    fn reconstruct_token_ownership(
        &self,
        tx: &mut WriteTransaction,
        score_threshold: u64,
        premint_from_removed: AHashMap<Tick, u64>,
    ) -> krc721_database::result::Result<()> {
        // Get affected ownership changes
        let affected_tokens =
            self.get_affected_token_operations(tx, score_threshold, premint_from_removed)?;

        // Remove current ownership records
        self.remove_ownership_records(tx, affected_tokens.iter().map(|(tmk, _)| tmk))?;

        // Reconstruct ownership from history
        self.rebuild_ownership_state(tx, affected_tokens)?;

        Ok(())
    }

    /// Retrieve token operations affected by the reorg based on score threshold.
    fn get_affected_token_operations(
        &self,
        tx: &mut WriteTransaction,
        score_threshold: u64,
        premint_from_removed: AHashMap<Tick, u64>,
    ) -> krc721_database::result::Result<Vec<(TokenMintsKey, Option<ScriptPublicKey>)>> {
        let affected_keys = self
            .db
            .ownership_changes
            .range_keys_wtx(
                tx,
                TokenMintsKey {
                    score: score_threshold,
                    tick: Tick::MIN,
                    token_id: 0,
                    reversed_seq_number: 0,
                }..,
            )
            .collect::<Result<Vec<_>, _>>()?;
        if !affected_keys.is_empty() {
            warn!("Affected keys: {:?}", affected_keys);
        }

        // Now process each key with a new borrow of tx
        affected_keys
            .into_iter()
            .rev()
            .map(|k| {
                let holder = self.db.ownership_history.remove_if_exists_wtx(
                    tx,
                    &OwnershipHistoryKey::with_score(k.tick, k.token_id, k.score),
                )?;
                self.db.ownership_changes.remove_wtx(tx, &k)?;
                if (self.db.mint_history.remove_if_exists_wtx(
                    tx,
                    &MintHistoryKey::new(k.tick, k.reversed_seq_number, k.token_id, k.score),
                )?)
                .is_some()
                {
                    let premint_data = {
                        if let Some(premint_data) = premint_from_removed.get(&k.tick) {
                            premint_data
                        } else {
                            &self
                                .db
                                .collection_registry
                                .get_wtx(tx, &k.tick)?
                                .map(|d| d.info.info.premint)
                                .expect("Fallback should not fail")
                        }
                    };
                    if k.token_id > *premint_data {
                        self.rollback_token_generation(tx, &k.tick, k.token_id)
                            .expect("Rollback should never fail");
                    }
                }
                Ok((k, holder))
            })
            .collect()
    }

    /// Remove current ownership records for the given tokens.
    fn remove_ownership_records<'a>(
        &self,
        tx: &mut WriteTransaction,
        affected_tokens: impl IntoIterator<Item = &'a TokenMintsKey>,
    ) -> krc721_database::result::Result<()> {
        for key in affected_tokens.into_iter() {
            self.db.current_ownership.remove_wtx(
                tx,
                &OwnershipKey {
                    tick: key.tick,
                    token_id: key.token_id,
                },
            )?;
        }
        Ok(())
    }

    /// Rebuild token ownership state from valid history entries.
    /// For each token:
    /// 1. Remove old address holdings
    /// 2. Find latest valid owner from history
    /// 3. Update current ownership and address holdings
    fn rebuild_ownership_state(
        &self,
        tx: &mut WriteTransaction,
        affected_tokens: Vec<(TokenMintsKey, Option<ScriptPublicKey>)>,
    ) -> krc721_database::result::Result<()> {
        for (key, holder) in affected_tokens {
            if let Some(spk) = holder {
                self.db.address_holdings.remove_wtx(
                    tx,
                    &AddressHoldingKey {
                        spk,
                        tick: key.tick,
                        token_id: key.token_id,
                    },
                )?;
            }

            // Reconstruct from latest valid history
            let token_key = OwnershipKey {
                tick: key.tick,
                token_id: key.token_id,
            };
            if let Some((owner, mod_tx_score)) = self
                .db
                .ownership_history
                .last_owner_with_tx_mod_score_wtx(tx, &token_key)?
            {
                self.db.current_ownership.insert_wtx(
                    tx,
                    token_key,
                    &CurrentOwnershipValue {
                        owner: owner.clone(),
                        mod_tx_score,
                    },
                )?;

                self.db.address_holdings.insert_wtx(
                    tx,
                    AddressHoldingKey {
                        spk: owner,
                        tick: key.tick,
                        token_id: key.token_id,
                    },
                    &mod_tx_score,
                )?;
            }
        }
        Ok(())
    }

    /// Process collection deployment operations by validating deployment parameters and recording
    /// the deployment if valid.
    fn process_deployment(
        &self,
        wtx: &mut WriteTransaction,
        tx_score: u64,
        common: &OperationCommon,
        info: &DeployInfo,
    ) -> Result<Result<(), CtxValidationError>> {
        let collection_deployments = &self.db.collection_deployments;
        let collection_registry = &self.db.collection_registry;

        if let Some(_royalty) = &info.royalty {
            // todo: royalty validation
        }

        match collection_registry.get_wtx(wtx, &common.tick)? {
            Some(ScoredDeployInfoWithCommon { score, .. }) if score >= tx_score => {
                tracing::error!("Collection registry must not have tick with score: {score} which is greater than incoming tx score: {tx_score}");
                Err(Error::UnexpectedKaspaNodeBehaviour)
            }
            Some(_) => Ok(Err(CtxValidationError::TickExists)),
            None => {
                collection_registry.insert_wtx(
                    wtx,
                    common.tick,
                    &ScoredDeployInfoWithCommon {
                        score: tx_score,
                        info: DeployInfoWithCommon {
                            info: info.clone(),     // todo impl referential deployinfo
                            common: common.clone(), // todo impl referential deployinfo
                        },
                    },
                )?;
                collection_deployments.insert_wtx(
                    wtx,
                    DeploymentKey {
                        score: tx_score,
                        tick: common.tick,
                    },
                    &DeployInfoWithCommon {
                        info: info.clone(),     // todo impl referential deployinfo
                        common: common.clone(), // todo impl referential deployinfo
                    },
                )?;

                if info.premint > 0 {
                    for _ in 1..=info.premint {
                        match self.process_mint(
                            wtx,
                            0,
                            tx_score,
                            common,
                            &mut MintInfo {
                                token_id: 0,
                                to: info.deployer.clone(),
                                royalty: None,
                            },
                            true,
                        ) {
                            Ok(Ok(())) => (),
                            Ok(Err(validation_err)) => return Ok(Err(validation_err)),
                            Err(err) => return Err(err),
                        }
                    }
                }

                Ok(Ok(()))
            }
        }
    }

    #[instrument(skip(self, wtx), fields(score = score, tick = %tick))]
    fn process_mint(
        &self,
        wtx: &mut WriteTransaction,
        mergeset_entropy: u64,
        score: u64,
        OperationCommon {
            tick,
            accepting_block_daa_score,
            ..
        }: &OperationCommon,
        MintInfo {
            to,
            royalty,
            token_id,
        }: &mut MintInfo,
        is_deployment: bool,
    ) -> Result<Result<(), CtxValidationError>> {
        let Some(ScoredDeployInfoWithCommon {
            info:
                DeployInfoWithCommon {
                    info:
                        DeployInfo {
                            max: max_supply,
                            royalty: deploy_royalty,
                            mint_start_daa,
                            premint,
                            ..
                        },
                    ..
                },
            ..
        }) = self.db.collection_registry.get_wtx(wtx, tick)?
        else {
            return Ok(Err(CtxValidationError::TickNotFound));
        };
        if !is_deployment && accepting_block_daa_score < &mint_start_daa {
            return Ok(Err(CtxValidationError::MintingNotStarted {
                tick: *tick,
                current_accepting_block_daa_score: *accepting_block_daa_score,
                start_accepting_block_daa_score: mint_start_daa,
            }));
        }
        let Some(max_supply) = NonZeroU64::new(max_supply) else {
            return Ok(Err(CtxValidationError::MintingFinished));
        };

        let last_mint = self
            .db
            .mint_history
            .last_minted_token_seq_no_wtx(wtx, tick)?
            .map(|t| t.seq_no)
            .unwrap_or_default();

        let should_premint = premint > 0 && last_mint < premint;

        // Royalties validation.
        if !should_premint {
            match (deploy_royalty, royalty) {
                (Some(deploy_royalty), Some(mint_royalty)) => {
                    let required_fee = self
                        .db
                        .vip
                        .last_fee_wtx(wtx, to, tick)?
                        .unwrap_or(deploy_royalty.fee); // Validate mint royalty against collection royalty rule
                    if required_fee > mint_royalty.fee {
                        return Ok(Err(CtxValidationError::InsufficientRoyaltyFee));
                    }
                    if deploy_royalty.beneficiary != mint_royalty.beneficiary {
                        return Ok(Err(CtxValidationError::InvalidBeneficiaryForRoyaltyMintFee));
                    }
                }
                (Some(_deploy_royalty), None) => {
                    return Ok(Err(CtxValidationError::MissingRoyaltyMintFee));
                }
                (None, _) => {
                    // No royalties configured. Minting is allowed.
                }
            }
        }

        let token_seq_no = last_mint + 1;

        if token_seq_no > max_supply.get() {
            debug!("Minting is finished but minting is requested");
            return Ok(Err(CtxValidationError::MintingFinished));
        }

        if !should_premint && premint < max_supply.get() {
            let rand_token_id =
                self.generate_token_id(wtx, tick, mergeset_entropy, max_supply, premint)?;
            let rand_token_id = rand_token_id.get();
            // Update original tokenid
            *token_id = rand_token_id;
        } else {
            *token_id = token_seq_no;
        }

        self.db.mint_history.insert_wtx(
            wtx,
            MintHistoryKey::with_seq(*tick, token_seq_no, *token_id, score),
            &(),
        )?;
        self.db.ownership_changes.insert_wtx(
            wtx,
            TokenMintsKey::with_seq(score, *tick, *token_id, token_seq_no),
            &(),
        )?;
        self.db.ownership_history.insert_wtx(
            wtx,
            OwnershipHistoryKey::with_score(*tick, *token_id, score),
            to,
        )?;
        self.db.current_ownership.insert_wtx(
            wtx,
            OwnershipKey {
                tick: *tick,
                token_id: *token_id,
            },
            &CurrentOwnershipValue {
                owner: to.clone(),
                mod_tx_score: score,
            },
        )?;
        self.db.address_holdings.insert_wtx(
            wtx,
            AddressHoldingKey {
                spk: to.clone(),
                tick: *tick,
                token_id: *token_id,
            },
            &score,
        )?;

        Ok(Ok(()))
    }

    fn process_transfer(
        &self,
        wtx: &mut WriteTransaction,
        tx_score: u64,
        OperationCommon { tick, sender, .. }: &OperationCommon,
        TransferInfo { token_id, to }: &TransferInfo,
    ) -> Result<Result<(), CtxValidationError>> {
        let owner = self.db.current_ownership.get_wtx(
            wtx,
            &OwnershipKey {
                tick: *tick,
                token_id: *token_id,
            },
        )?;
        let owner = match owner {
            None => return Ok(Err(CtxValidationError::TokenNotFound)),
            Some(CurrentOwnershipValue { owner, .. }) if &owner != sender => {
                return Ok(Err(CtxValidationError::WrongOwner));
            }
            Some(owner) => owner.owner,
        };

        // Block transfer if token is listed for sale
        if self
            .db
            .listings
            .get_wtx(
                wtx,
                &OwnershipKey {
                    tick: *tick,
                    token_id: *token_id,
                },
            )?
            .is_some()
        {
            return Ok(Err(CtxValidationError::TokenIsListed));
        }

        self.db.ownership_changes.insert_wtx(
            wtx,
            TokenMintsKey::with_seq(tx_score, *tick, *token_id, 0),
            &(),
        )?;
        self.db.ownership_history.insert_wtx(
            wtx,
            OwnershipHistoryKey::with_score(*tick, *token_id, tx_score),
            to,
        )?;
        self.db.current_ownership.insert_wtx(
            wtx,
            OwnershipKey {
                tick: *tick,
                token_id: *token_id,
            },
            &CurrentOwnershipValue {
                owner: to.clone(),
                mod_tx_score: tx_score,
            },
        )?;

        self.db.address_holdings.remove_wtx(
            wtx,
            &AddressHoldingKey {
                spk: owner,
                tick: *tick,
                token_id: *token_id,
            },
        )?;

        self.db.address_holdings.insert_wtx(
            wtx,
            AddressHoldingKey {
                spk: to.clone(),
                tick: *tick,
                token_id: *token_id,
            },
            &tx_score,
        )?;
        Ok(Ok(()))
    }

    #[instrument(skip(self))]
    fn process_queue(&self, vcc: VirtualChainChanges) -> Result<()> {
        let mut wtx = self.db.write_tx();
        let next_key = self
            .db
            .notification_queue
            .last_key_wtx(&mut wtx)?
            .unwrap_or_default()
            + 1;
        self.db
            .notification_queue
            .insert_wtx(&mut wtx, next_key, &vcc)?;
        wtx.commit()
            .map_err(krc721_database::error::Error::Fjall)?
            .expect("no conflict should happen");
        Ok(())
    }

    #[instrument(skip(self))]
    fn process_queue_application(&self) -> Result<()> {
        let mut wtx = self.db.write_tx();
        let first = self
            .db
            .notification_queue
            .first_key_wtx(&mut wtx)?
            .unwrap_or_default();
        let last = self
            .db
            .notification_queue
            .last_key_wtx(&mut wtx)?
            .unwrap_or_default();
        let _span = tracing::span!(
            tracing::Level::INFO,
            "process_queue_application",
            first = first,
            last = last
        )
        .entered();
        debug_assert!(
            first <= last,
            "first key must be less than or equal to last key"
        );
        if first * last == 0 {
            warn!("Empty queue, nothing to process. Can happen during fast catch up.");
            return Ok(());
        }
        for i in first..=last {
            let vcc = self
                .db
                .notification_queue
                .remove_if_exists_wtx(&mut wtx, &i)?
                .unwrap();
            self.process_chain_changes_wtx(vcc, &mut wtx)
                .inspect(|_| info!("queue change {i} is processed successfully"))
                .inspect_err(|err| error!("queue change {i} failed: {err}"))?;
        }
        wtx.commit()
            .map_err(krc721_database::error::Error::Fjall)?
            .expect("no conflict should happen");
        info!("queue application is finished successfully");
        Ok(())
    }

    fn process_discount(
        &self,
        wtx: &mut WriteTransaction,
        score: u64,
        common: &OperationCommon,
        info: &mut DiscountInfo,
    ) -> Result<Result<(), CtxValidationError>> {
        let Some(deployment) = self.db.collection_registry.get_wtx(wtx, &common.tick)? else {
            return Ok(Err(CtxValidationError::TickNotFound));
        };
        let deployer = deployment.info.info.deployer;
        if common.sender != deployer {
            return Ok(Err(CtxValidationError::WrongDeployer));
        }
        if info.fee
            >= deployment
                .info
                .info
                .royalty
                .map(|r| r.fee)
                .unwrap_or_default()
        {
            return Ok(Err(CtxValidationError::DiscountFeeOverflow));
        }

        self.db.score_to_discount.insert_wtx(
            wtx,
            ScoredDiscountKey {
                score,
                tick: common.tick,
                spk: info.to.clone(),
            },
            &info.fee,
        )?;
        self.db.vip.insert_wtx(
            wtx,
            VipKey {
                spk: info.to.clone(),
                tick: common.tick,
                reversed_score: u64::MAX - score,
                fee: info.fee,
            },
            &(),
        )?;
        Ok(Ok(()))
    }

    fn process_list(
        &self,
        wtx: &mut WriteTransaction,
        tx_score: u64,
        common: &OperationCommon,
        info: &ListingInfo,
    ) -> Result<Result<(), CtxValidationError>> {
        use krc721_core::constants::MIN_LISTING_PRICE;

        // Validate price
        if info.price < MIN_LISTING_PRICE {
            return Ok(Err(CtxValidationError::InvalidListingPrice));
        }

        // Validate tick exists
        if self
            .db
            .collection_registry
            .get_wtx(wtx, &common.tick)?
            .is_none()
        {
            return Ok(Err(CtxValidationError::TickNotFound));
        }

        // Validate sender owns the token
        let owner = self.db.current_ownership.get_wtx(
            wtx,
            &OwnershipKey {
                tick: common.tick,
                token_id: info.token_id,
            },
        )?;
        match &owner {
            None => return Ok(Err(CtxValidationError::TokenNotFound)),
            Some(CurrentOwnershipValue { owner, .. }) if owner != &common.sender => {
                return Ok(Err(CtxValidationError::WrongOwner));
            }
            _ => {}
        }

        // Validate token is not already listed
        let listing_key = OwnershipKey {
            tick: common.tick,
            token_id: info.token_id,
        };
        if self.db.listings.get_wtx(wtx, &listing_key)?.is_some() {
            return Ok(Err(CtxValidationError::TokenAlreadyListed));
        }

        // Store the listing
        let listing_value = ListingValue {
            price: info.price,
            seller: common.sender.clone(),
            listing_tx_id: common.tx_id,
            utxo_address: info.utxo_address.clone(),
            redeem_script: info.redeem_script.clone(),
            op_score: tx_score,
        };

        self.db.listings.insert_wtx(wtx, listing_key, &listing_value)?;

        // Sorted marketplace index
        self.db.listings_by_tick.insert_wtx(
            wtx,
            ListingByTickKey {
                tick: common.tick,
                price: info.price,
                token_id: info.token_id,
            },
            &(),
        )?;

        // Seller's listings index
        self.db.address_listings.insert_wtx(
            wtx,
            AddressHoldingKey {
                spk: common.sender.clone(),
                tick: common.tick,
                token_id: info.token_id,
            },
            &tx_score,
        )?;

        Ok(Ok(()))
    }

    fn process_send(
        &self,
        wtx: &mut WriteTransaction,
        tx_score: u64,
        common: &OperationCommon,
        info: &SendInfo,
    ) -> Result<Result<(), CtxValidationError>> {
        let listing_key = OwnershipKey {
            tick: common.tick,
            token_id: info.token_id,
        };

        // Validate listing exists
        let listing = match self.db.listings.get_wtx(wtx, &listing_key)? {
            None => return Ok(Err(CtxValidationError::ListingNotFound)),
            Some(listing) => listing,
        };

        // Validate input[0] spends the listing UTXO
        if info.listing_utxo_txid != listing.listing_tx_id {
            return Ok(Err(CtxValidationError::WrongListingUtxo));
        }

        // Validate payment >= listing price
        if info.payment_amount < listing.price {
            return Ok(Err(CtxValidationError::InsufficientPayment));
        }

        // Transfer ownership from seller to buyer (same logic as process_transfer)
        let tick = common.tick;
        let token_id = info.token_id;
        let seller = listing.seller.clone();
        let buyer = &info.buyer;

        self.db.ownership_changes.insert_wtx(
            wtx,
            TokenMintsKey::with_seq(tx_score, tick, token_id, 0),
            &(),
        )?;
        self.db.ownership_history.insert_wtx(
            wtx,
            OwnershipHistoryKey::with_score(tick, token_id, tx_score),
            buyer,
        )?;
        self.db.current_ownership.insert_wtx(
            wtx,
            OwnershipKey { tick, token_id },
            &CurrentOwnershipValue {
                owner: buyer.clone(),
                mod_tx_score: tx_score,
            },
        )?;
        self.db.address_holdings.remove_wtx(
            wtx,
            &AddressHoldingKey {
                spk: seller.clone(),
                tick,
                token_id,
            },
        )?;
        self.db.address_holdings.insert_wtx(
            wtx,
            AddressHoldingKey {
                spk: buyer.clone(),
                tick,
                token_id,
            },
            &tx_score,
        )?;

        // Clean up listing state
        self.db.listings.remove_wtx(wtx, &listing_key)?;
        self.db.listings_by_tick.remove_wtx(
            wtx,
            &ListingByTickKey {
                tick,
                price: listing.price,
                token_id,
            },
        )?;
        self.db.address_listings.remove_wtx(
            wtx,
            &AddressHoldingKey {
                spk: seller,
                tick,
                token_id,
            },
        )?;
        Ok(Ok(()))
    }

    fn remove_discounts(&self, wtx: &mut WriteTransaction, tx_score_threshold: u64) -> Result<()> {
        let discounts = self
            .db
            .score_to_discount
            .range_from_score_wtx(wtx, tx_score_threshold)
            .collect::<Result<Vec<_>, krc721_database::error::Error>>()?;

        for (ScoredDiscountKey { score, tick, spk }, fee) in discounts {
            self.db.vip.remove_wtx(
                wtx,
                &VipKey {
                    spk,
                    tick,
                    reversed_score: u64::MAX - score,
                    fee,
                },
            )?;
        }
        Ok(())
    }
}

const SERVICE: &str = "PROC";

#[async_trait]
impl Service for Processor {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        let this = self.clone();
        let rt_thread = thread::Builder::new()
            .name("realtime-ingest".to_string())
            .spawn(move || {
                _ = this
                    .rtvccn_task()
                    .inspect_err(|err| error!("{SERVICE} error: {err}"));
            })
            .expect("failed to spawn ingest thread");
        let this = self.clone();
        let ht_thread = thread::Builder::new()
            .name("historical-ingest".to_string())
            .spawn(move || {
                _ = this
                    .htvccn_task()
                    .inspect_err(|err| error!("{SERVICE} error: {err}"));
            })
            .expect("failed to spawn ingest thread");
        self.ingest_thread_handle
            .lock()
            .unwrap()
            .replace((rt_thread, ht_thread));
        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        self.shutdown_sender.send(()).unwrap();
        self.shutdown_sender.send(()).unwrap();
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        let thread_handles = self.ingest_thread_handle.lock().unwrap().take();
        if let Some((rt_thread, ht_thread)) = thread_handles {
            spawn_blocking(move || {
                rt_thread.join().unwrap();
                ht_thread.join().unwrap();
            })
            .await
            .map_err(ServiceError::custom)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ContextOperation {
    pub tx_score: u64,
    pub operation: Operation,
    pub mergeset_entropy: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_processor_shutdown() {
        use krc721_core::network::Network;
        use std::fs;

        let runtime = Runtime::default();
        let db_folder = dirs::home_dir().unwrap().join("krc721d_processor_tests");
        _ = fs::remove_dir_all(&db_folder);
        let db = Arc::new(Db::try_open(&db_folder, &Network::Mainnet).unwrap());
        let counters = Arc::new(Counters::default());
        let processor: Arc<dyn Service> = Arc::new(Processor::new(db, counters, None, None));
        runtime.bind(processor);
        tokio::spawn({
            let rt = runtime.clone();
            async move {
                rt.run().await.unwrap();
            }
        });
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        runtime.terminate();
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        _ = fs::remove_dir_all(&db_folder);
    }
}
