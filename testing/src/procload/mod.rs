mod tokengen;
use crate::error::Error;
use ahash::{AHashMap, AHashSet};
use async_std::channel::Receiver;
use async_trait::async_trait;
use core::panic;
use futures::{select_biased, FutureExt};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionId};
use kaspa_txscript::pay_to_address_script;
use krc721_core::model::krc721::ScoredCheckedOperation;
use krc721_core::{
    model::krc721::{
        DeployInfo, Mergeset, MergesetOperation, Metadata, MintInfo, Operation, OperationCommon,
        OperationInfo, Tick, TransferInfo, VirtualChainChanges, TICK_LENGTH,
    },
    result::Result as ServiceResult,
    runtime::{Runtime, Service},
};
use krc721_database::prelude::Db;
use krc721_nexus::prelude::{Metrics, Processor};
use rand::Rng;
use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokengen::TokenGen;
use tracing::{error, info, info_span, warn, Instrument};
pub use workflow_core::channel::{oneshot, DuplexChannel, Multiplexer, Sender};
use workflow_core::task;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum OperationType {
    Deploy,
    Transfer,
    Mint,
}

#[derive(Debug, Clone)]
pub struct CollectionConfig {
    pub min_size: u64,
    pub max_size: u64,
}

#[derive(Debug, Clone)]
pub struct OperationConfig {
    pub include_deploy: bool,
    pub include_mint: bool,
    pub include_transfer: bool,
    pub transfers_per_deploy: u64,
}

#[derive(Debug, Clone)]
pub struct ReorgConfig {
    pub reorg_batch_frequency: u32,
    pub reorg_depth: u32,
}

#[derive(Clone)]
pub struct ProcLoadState {
    pub transferable_tokens: VecDeque<TokenOwnership>,
    pub collections: VecDeque<CollectionInfo>,
    pub deployed_collections: u128,
    pub mints: u128,
    pub transfers: u128,
    pub current_blue_score: u64,
    pub collection_supplies: AHashMap<Tick, u64>,
    pub token_gen: TokenGen,
}
impl ProcLoadState {
    fn from_data(data: &Data) -> Self {
        Self {
            transferable_tokens: data.transferable_tokens.clone(),
            collections: data.collections.clone(),
            deployed_collections: data.deployed_collections,
            mints: data.mints,
            transfers: data.transfers,
            current_blue_score: data.current_blue_score,
            collection_supplies: data.collection_supplies.clone(),
            token_gen: data.token_gen.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadTestConfig {
    pub collection_config: CollectionConfig,
    pub operation_config: OperationConfig,
    pub reorg_config: ReorgConfig,
    pub deploy_count: usize,
    pub mergeset_count: usize,
    pub ops_per_mergeset: usize,
    pub use_local_metadata: bool,
    pub starting_block_time: u64,
    pub max_batches: Option<u64>,
    pub overfill_mint: bool,
}

#[derive(Debug, Clone)]
pub struct CollectionInfo {
    tick: Tick,
    max_supply: u64,
    minted: u64,
    mint_attempts: u64,
    premint: u64,
}

#[derive(Debug, Clone)]
pub struct TokenOwnership {
    pub collection: Tick,
    pub token_id: u64,
    pub owner: ScriptPublicKey,
    pub mod_score: u64,
}

pub struct PendingOperation {
    pub op_type: OperationType,
    pub tx_id: TransactionId,
    pub token: Option<TokenOwnership>,
}

pub struct Batch {
    pub start_time: Instant,
    pub ops: u64,
}

pub struct Data {
    pub transferable_tokens: VecDeque<TokenOwnership>,
    pub collections: VecDeque<CollectionInfo>,
    pub collection_supplies: AHashMap<Tick, u64>,
    pub previous_vccs: VecDeque<(VirtualChainChanges, ProcLoadState)>, // Store state with each VCC
    pub completed_operations: u128,
    pub deployed_collections: u128,
    pub transfers: u128,
    pub mints: u128,
    pub start_time: Instant,
    pub active_processing_time: Duration,
    pub active_batches: VecDeque<Batch>,
    pub completed_batches: u64,
    pub current_blue_score: u64,
    pub last_reorg: u32,
    pub token_gen: TokenGen,
}

pub struct ProcLoad {
    runtime: Arc<Runtime>,
    shutdown: DuplexChannel<()>,
    #[allow(unused)]
    db: Arc<Db>,
    processor: Arc<Processor>,
    config: LoadTestConfig,
    data: Arc<RwLock<Data>>,
    op_complete_notif: Receiver<ScoredCheckedOperation>,
    tx_write_notif: Receiver<()>,
    metrics: Arc<Metrics>,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            overfill_mint: false,
            max_batches: None,
            collection_config: CollectionConfig {
                min_size: 100,
                max_size: 1000,
            },
            operation_config: OperationConfig {
                include_deploy: true,
                include_mint: true,
                include_transfer: true,
                transfers_per_deploy: 100,
            },
            reorg_config: ReorgConfig {
                reorg_depth: 0,
                reorg_batch_frequency: 0,
            },
            deploy_count: 1000,
            mergeset_count: 10,
            ops_per_mergeset: 5,
            use_local_metadata: false,
            starting_block_time: 1736356739,
        }
    }
}

impl ProcLoad {
    pub fn new(
        runtime: Arc<Runtime>,
        processor: Arc<Processor>,
        db: Arc<Db>,
        config: LoadTestConfig,
        receiver: Receiver<ScoredCheckedOperation>,
        tx_write_receiver: Receiver<()>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            runtime,
            shutdown: DuplexChannel::oneshot(),
            processor,
            db,
            config: config.clone(),
            data: Arc::new(RwLock::new(Data {
                token_gen: TokenGen::new(),
                collection_supplies: AHashMap::new(),
                previous_vccs: VecDeque::new(),
                completed_operations: 0,
                deployed_collections: 0,
                transfers: 0,
                mints: 0,
                start_time: Instant::now(),
                active_processing_time: Duration::from_secs(0),
                completed_batches: 0,
                current_blue_score: config.starting_block_time,
                collections: VecDeque::new(),
                transferable_tokens: VecDeque::new(),
                active_batches: VecDeque::new(),
                last_reorg: 0,
            })),
            op_complete_notif: receiver,
            metrics,
            tx_write_notif: tx_write_receiver,
        }
    }

    fn get_next_operation(&self) -> Option<OperationType> {
        let data = self.data.read().unwrap();
        let mut rng = rand::thread_rng();

        // If there are no collections, we must deploy first
        if data.collections.is_empty()
            && data.deployed_collections < self.config.deploy_count as u128
        {
            return Some(OperationType::Deploy);
        }

        // Get collections that have minted tokens for transfer
        let available_for_transfer = !data.transferable_tokens.is_empty();

        let mut available_ops = Vec::new();
        // Get collections that haven't reached max supply for minting
        let available_for_mint = data.collections.len();

        // If no collections are available for minting, we must deploy
        if available_for_mint == 0 && data.deployed_collections < self.config.deploy_count as u128 {
            return Some(OperationType::Deploy);
        }

        // If collections exist for minting, only 10% chance to include deploy
        if self.config.operation_config.include_deploy
            && rng.gen_bool(0.1)
            && data.deployed_collections < self.config.deploy_count as u128
        {
            available_ops.push(OperationType::Deploy);
        }

        // Add mint if we have collections with space
        if self.config.operation_config.include_mint && available_for_mint > 0 {
            available_ops.push(OperationType::Mint);
        }

        // Add transfer if we have tokens to transfer
        if self.config.operation_config.include_transfer && available_for_transfer {
            available_ops.push(OperationType::Transfer);
        }

        if available_ops.is_empty() {
            unreachable!("No operations available");
        } else {
            Some(available_ops[rng.gen_range(0..available_ops.len())].clone())
        }
    }

    fn get_mergeset(&self) -> Mergeset {
        let mut mergeset_ops = Vec::new();
        let current_blue_score = self.data.read().unwrap().current_blue_score;
        let mergeset_entropy: u64 = rand::random();
        for i in 0..self.config.ops_per_mergeset {
            if let Some(op_type) = self.get_next_operation() {
                let op = match op_type {
                    OperationType::Deploy => self.create_deploy_operation(current_blue_score),
                    OperationType::Mint => {
                        if let Some(op) =
                            self.create_mint_operation(current_blue_score, mergeset_entropy)
                        {
                            op
                        } else {
                            continue;
                        }
                    }
                    OperationType::Transfer => {
                        if let Some(op) = self.create_transfer_operation(current_blue_score) {
                            op
                        } else {
                            continue;
                        }
                    }
                };
                mergeset_ops.push(MergesetOperation {
                    block_index_within_mergeset: 0,
                    operation: op,
                    index_within_merged_block: i,
                });
            }
        }

        Mergeset {
            operations: mergeset_ops,
            entropy: mergeset_entropy,
            blue_score: current_blue_score,
            accepted_chain_block_hash: Self::generate_random_hash().into(),
        }
    }

    fn create_deploy_operation(&self, current_blue_score: u64) -> Operation {
        // Generate random ticker, deployer, and size

        let tick = self.generate_tick();

        let (_, script) = Self::generate_random_address();

        let size = rand::thread_rng().gen_range(
            self.config.collection_config.min_size..=self.config.collection_config.max_size,
        );

        // Randomly select if premint
        let mut rng = rand::thread_rng();
        let do_premint = rng.gen_bool(0.1);

        // Randomly select the number of tokens to premint
        let to_premint = {
            if do_premint {
                rng.gen_range(1..=size)
            } else {
                0
            }
        };

        let tx_id = Self::generate_random_hash().into();
        let mut data = self.data.write().unwrap();

        // Track the new collection
        data.collections.push_back(CollectionInfo {
            tick,
            max_supply: size,
            mint_attempts: 0,
            minted: to_premint,
            premint: to_premint,
        });

        // Track the preminted tokens
        for i in 1..=to_premint {
            data.transferable_tokens.push_back(TokenOwnership {
                collection: tick,
                token_id: i,
                owner: script.clone(),
                mod_score: current_blue_score,
            });
            data.mints += 1;
        }

        // Track the collection supply
        data.collection_supplies.insert(tick, size);

        Operation {
            common: OperationCommon {
                tick,
                tx_id,
                block_time: current_blue_score,
                sender: script.clone(),
                fee: 1000,
                accepting_block_daa_score: current_blue_score,
            },
            info: OperationInfo::Deploy(DeployInfo {
                metadata: Metadata::Remote("krc721://test.com".to_string()),
                max: size,
                deployer: script.clone(),
                royalty: None,
                mint_start_daa: 0,
                premint: to_premint,
            }),
        }
    }

    fn create_mint_operation(
        &self,
        current_blue_score: u64,
        mergeset_entropy: u64,
    ) -> Option<Operation> {
        let mut data = self.data.write().unwrap();

        if data.collections.is_empty() {
            return None;
        }

        let collection_info = {
            let collection = data.collections.front().unwrap();
            (
                collection.tick,
                collection.max_supply,
                collection.mint_attempts,
                collection.minted,
                collection.premint,
            )
        };
        let (collection_tick, max_supply, current_attempts, current_minted, premint) =
            collection_info;

        let token_id = if current_minted < max_supply {
            data.token_gen
                .generate(&collection_tick, mergeset_entropy, max_supply, premint)
        } else {
            1
        };

        let (_, script) = Self::generate_random_address();

        // Track real mints separately from attempts
        if current_minted < max_supply {
            data.transferable_tokens.push_back(TokenOwnership {
                collection: collection_tick,
                token_id,
                owner: script.clone(),
                mod_score: current_blue_score,
            });
            data.mints += 1;

            // Increment actual mints
            if let Some(collection) = data.collections.front_mut() {
                collection.minted += 1;
            }
        }

        // Always increment attempt counter
        if let Some(collection) = data.collections.front_mut() {
            collection.mint_attempts += 1;
        }

        // Handle collection completion - when overfill disabled, check actual mints
        let should_remove = if self.config.overfill_mint {
            current_attempts + 1 >= max_supply + 2
        } else {
            current_minted + 1 >= max_supply
        };

        if should_remove {
            data.collections.pop_front();
        }

        Some(Operation {
            common: OperationCommon {
                tick: collection_tick,
                tx_id: Self::generate_random_hash().into(),
                block_time: current_blue_score,
                sender: script.clone(),
                fee: 1000,
                accepting_block_daa_score: current_blue_score,
            },
            info: OperationInfo::Mint(MintInfo {
                token_id,
                to: script,
                royalty: None,
            }),
        })
    }

    fn create_transfer_operation(&self, current_blue_score: u64) -> Option<Operation> {
        let mut data = self.data.write().unwrap();

        // Get a random token from transferable set
        let token = data.transferable_tokens.pop_front()?;

        let token_id = token.token_id;

        let old_owner = token.owner.clone();
        let collection = token.collection;

        // Generate new owner
        let (_, to_script) = Self::generate_random_address();

        // Create updated token ownership
        let mut new_token = token;
        new_token.owner = to_script.clone();
        // Update the ownership
        data.transferable_tokens.push_back(new_token);
        data.transfers += 1;
        let tx_id = Self::generate_random_hash().into();

        Some(Operation {
            common: OperationCommon {
                tick: collection,
                tx_id,
                block_time: current_blue_score,
                sender: old_owner,
                fee: 1000,
                accepting_block_daa_score: current_blue_score,
            },
            info: OperationInfo::Transfer(TransferInfo {
                token_id,
                to: to_script,
            }),
        })
    }

    fn generate_tick(&self) -> Tick {
        let mut data = self.data.write().unwrap();

        data.deployed_collections += 1;
        let raw_tick = data.deployed_collections;

        drop(data);

        let mut tick_bytes = [0u8; TICK_LENGTH];
        // Convert number to string and then to ASCII bytes
        let tick_str = raw_tick.to_string();
        let ascii_bytes = tick_str.as_bytes();

        // Copy the ASCII bytes to tick_bytes, padding with zeros if needed
        let copy_len = ascii_bytes.len().min(TICK_LENGTH);
        tick_bytes[..copy_len].copy_from_slice(&ascii_bytes[..copy_len]);

        Tick(tick_bytes)
    }

    fn generate_random_address() -> (Address, ScriptPublicKey) {
        let mut rng = rand::thread_rng();
        let mut pubkey = [0u8; 32];
        rng.fill(&mut pubkey);

        let address = Address::new(Prefix::Testnet, Version::PubKey, &pubkey);
        let script = pay_to_address_script(&address);
        (address, script)
    }

    fn generate_random_hash() -> [u8; 32] {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes);
        bytes
    }

    async fn handle_completed_operation(&self, op: ScoredCheckedOperation) {
        // If the operation failed, log the error and return
        if op.checked_operation.error.is_some() {
            warn!("Operation failed: {:?}", op.checked_operation.error);
            return;
        }

        // If the operation was successful, increment the completed operations count
        let mut data = self.data.write().unwrap();
        data.completed_operations += 1;
    }

    async fn handle_completed_batch(&self) {
        let mut data = self.data.write().unwrap();

        // If there are no active batches, return
        if data.active_batches.is_empty() {
            return;
        }

        // Pop the completed batch from the front
        let completed_batch = data.active_batches.pop_front().unwrap();
        let batch_duration = completed_batch.start_time.elapsed();

        // Update the active processing time and completed batches count
        data.active_processing_time += batch_duration;
        data.completed_batches += 1;

        // Calculate metrics
        let avg_ops_per_sec =
            data.completed_operations as f64 / data.active_processing_time.as_secs_f64();
        let batch_ops = completed_batch.ops as f64;
        let batch_ops_per_sec = batch_ops / batch_duration.as_secs_f64();

        info!(
                    "Batch completed - Batch Ops/s: {:.2}, Avg Ops/s: {:.2}, Active batches: {}, Deploy: {}, Mint: {}, Transfer: {}",
                    batch_ops_per_sec,
                    avg_ops_per_sec,
                    data.active_batches.len(),
                    data.deployed_collections,
                    data.mints,
                    data.transfers
                );

        #[allow(clippy::deprecated_cfg_attr)]
            #[cfg_attr(rustfmt, rustfmt_skip)]
            if let Some(client) = self.metrics.statsd() {
                client.gauge("procload.batch_ops", batch_ops);
                client.gauge("procload.batch_ops_per_sec", batch_ops_per_sec);
                client.gauge("procload.avg_ops_per_sec", avg_ops_per_sec);
                client.gauge("procload.active_batches", data.active_batches.len() as f64);
                client.gauge("procload.deployed_collections", data.deployed_collections as f64);
                client.gauge("procload.mints", data.mints as f64);
                client.gauge("procload.transfers", data.transfers as f64);
            }
    }

    fn should_reorg(&self) -> bool {
        // If reorgs are disabled, return None
        if self.config.reorg_config.reorg_batch_frequency == 0 {
            return false;
        }
        let data = self.data.read().unwrap();

        let batches_passed = data.completed_batches - data.last_reorg as u64;

        // Also check if we have enough VCCs stored
        if batches_passed >= self.config.reorg_config.reorg_batch_frequency as u64
            && data.previous_vccs.len() >= self.config.reorg_config.reorg_depth as usize
        {
            return true;
        }

        false
    }
    async fn do_ops(&self) {
        // Create a vec that will contain all the new mergesets
        let mut mergesets = Vec::with_capacity(self.config.mergeset_count);

        // Track all number of operations
        let mut ops = 0;

        // Check if we should do a reorg and get depth
        let should_reorg = self.should_reorg();
        let reorg_depth = self.config.reorg_config.reorg_depth;

        // Track all removed chain block hashes
        let mut removed_chain_block_hashes = Vec::new();

        // First capture the current state BEFORE any modifications
        // We only track when we should not reorg because the reorg
        // will invalidate the current state.
        let current_state = if !should_reorg {
            let data = self.data.read().unwrap();
            Some(ProcLoadState::from_data(&data))
        } else {
            None
        };

        let mut reorg_data = (Vec::new(), None);

        // Retrieve the data that should be reorged
        if should_reorg {
            let mut data = self.data.write().unwrap();

            // Only reorg if we have enough VCCs stored
            if reorg_depth as usize <= data.previous_vccs.len() {
                // Loop over for the depth of the reorg
                for _ in 0..reorg_depth {
                    // Pop the VCC and state from the front
                    let (vcc, state) = data.previous_vccs.pop_front().unwrap();
                    reorg_data.0.push(vcc);

                    // We only want the oldest state
                    if reorg_data.1.is_none() {
                        // This will be set only once.
                        reorg_data.1 = Some(state);
                    }
                }
            }
        }

        if let (vccs, Some(state)) = reorg_data {
            info!("Performing reorg of depth {}...", vccs.len());

            // Restore state first to the oldest state
            {
                let mut data = self.data.write().unwrap();
                data.transferable_tokens = state.transferable_tokens;
                data.collections = state.collections;
                data.deployed_collections = state.deployed_collections;
                data.mints = state.mints;
                data.transfers = state.transfers;
                data.last_reorg = data.completed_batches as u32;
                data.current_blue_score = state.current_blue_score;
                data.collection_supplies = state.collection_supplies;
                data.token_gen = state.token_gen;
            }

            // Generate new mergesets
            for vcc in vccs.iter() {
                for mergeset in vcc.mergesets.iter() {
                    // Track all removed chain block hashes
                    // but invert the order
                    removed_chain_block_hashes.insert(0, mergeset.accepted_chain_block_hash);

                    // Retrieve new operations
                    let mut new_mergeset = self.get_mergeset();
                    new_mergeset.blue_score = mergeset.blue_score;
                    {
                        // Increment blue score
                        let mut data = self.data.write().unwrap();
                        data.current_blue_score += 1;
                    }

                    // Track all number of operations
                    ops += new_mergeset.operations.len() as u64;
                    mergesets.push(new_mergeset);
                }
            }

            info!(
                "Reorg complete - Generated {} replacement mergesets",
                mergesets.len()
            );
        } else {
            // Normal operation - generate new mergesets
            for _ in 0..self.config.mergeset_count {
                let mergeset = self.get_mergeset();
                // update the current blue score
                {
                    let mut data = self.data.write().unwrap();
                    data.current_blue_score += 1;
                }
                // Track all number of operations
                ops += mergeset.operations.len() as u64;
                mergesets.push(mergeset);
            }
        }

        // Add new batch to active batches and store state
        {
            let mut data = self.data.write().unwrap();
            data.active_batches.push_back(Batch {
                start_time: Instant::now(),
                ops,
            });

            // Store the pre-modification state with the VCC
            // and only store if we are not reorging
            if let Some(pre_state) = current_state {
                let vcc = VirtualChainChanges {
                    removed_chain_block_hashes: Arc::new(Vec::new()),
                    forced_rollback_blue_score: None,
                    mergesets: mergesets.clone(),
                };

                data.previous_vccs.push_back((vcc, pre_state));

                while data.previous_vccs.len() > self.config.reorg_config.reorg_depth as usize {
                    data.previous_vccs.pop_front();
                }
            }
        }

        // Prepare the notification
        let notification = VirtualChainChanges {
            removed_chain_block_hashes: Arc::new(removed_chain_block_hashes),
            forced_rollback_blue_score: None,
            mergesets,
        };

        // Send the notification
        if let Err(e) = self
            .processor
            .send_realtime_virtual_chain_changed_notification(notification)
        {
            panic!("Failed to send mergesets: {}", e);
        }
    }

    async fn task(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        // Send out batches every 100ms to mimic tn11
        let mut operation_interval = tokio::time::interval(Duration::from_millis(100));

        'outer: loop {
            select_biased! {
                _ = operation_interval.tick().fuse() => {
                    // Lock batch count
                    let (completed_batches, active_batches) = {
                        let lock = self.data.read().unwrap();
                        (lock.completed_batches, lock.active_batches.len())
                    };
                    // If we have run out of batches, we should shut down
                    if let Some(max_batches) = self.config.max_batches {
                        if completed_batches >= max_batches {
                            info!("max batches reached - waiting for active batches to complete...");
                            if active_batches > 0 {
                                // There are still active batches, wait for them to complete
                                info!("Waiting for {} active batches to complete...", active_batches);
                            } else {
                                // All batches have completed, we can now shut down
                                break 'outer;
                            }
                        } else {
                            // If we haven't reached the max batches, we should continue
                            self.do_ops().await;
                        }
                    } else {
                        // If we don't have a max batch count, we should just continue
                        self.do_ops().await;
                    }
                },

                op = self.op_complete_notif.recv().fuse() => {
                    if let Ok(op) = op {
                        let self_clone = self.clone();
                        // If a completed operation was received, handle it in a separate task
                        task::spawn(async move {
                            self_clone.handle_completed_operation(op).await;
                        })
                    }
                },
                _ = self.tx_write_notif.recv().fuse() => {
                    // If a tx write notification is received, we should handle the completed batch
                    self.handle_completed_batch().await;
                },
                _ = self.shutdown.request.recv().fuse() => {
                    // In an infinite task we want to break out of the loop when we receive a shutdown signal
                    info!("Received shutdown signal ");
                    break;
                },
            }
        }

        if self.config.max_batches.is_some() {
            info!("All pending operations completed - verifying token holdings...");
            let mut data = self.data.write().unwrap();
            let rtx = self.db.read_tx();

            assert!(!data.transferable_tokens.is_empty(), "No tokens to verify");

            // Loop over all tokens and verify that they exist in the holdings
            // also check for duplicates and max supply
            let mut minted_tokens: AHashSet<(Tick, u64)> = AHashSet::new();

            // Verify that all tokens have been transferred
            assert_eq!(
                data.mints,
                data.transferable_tokens.len() as u128,
                "Transferred tokens count doesn't match minted tokens count"
            );

            // Remember the circulating supplies for each collection
            let mut circulating_supplies: AHashMap<Tick, u64> = AHashMap::new();

            // Go over all the transferable tokens
            while !data.transferable_tokens.is_empty() {
                // Pop the ownership from the front
                let ownership = data.transferable_tokens.pop_front().unwrap();

                // Verify that the token hasn't been minted before
                assert!(
                    !minted_tokens.contains(&(ownership.collection, ownership.token_id)),
                    "Duplicate token detected! - Collection: {:?}, Token ID: {}, Owner: {:?}, Score: {}",
                    ownership.collection,
                    ownership.token_id,
                    ownership.owner,
                    ownership.mod_score
                );

                // Insert the token into the minted tokens set
                minted_tokens.insert((ownership.collection, ownership.token_id));

                // Get the holdings for the token from the database
                let holdings_iter = self.db.address_holdings.address_holdings_by_tick_rtx(
                    &rtx,
                    &ownership.owner,
                    &ownership.collection,
                );

                // Collect the holdings into a vector
                let holdings: Vec<u64> = holdings_iter
                    .map(|result| result.map_err(Error::from))
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap();

                // Verify that the token_id exists in the holdings
                assert!(
                    holdings.contains(&ownership.token_id),
                    "Token verification failed - Collection: {:?}, Token ID: {}, Owner: {:?}, Score: {}",
                    ownership.collection,
                    ownership.token_id,
                    ownership.owner,
                    ownership.mod_score
                );

                // Retrieve the original max supply for the collection
                let max_supply = data
                    .collection_supplies
                    .get(&ownership.collection)
                    .unwrap_or(&0);

                // Retrieve the circulating supply for the collection
                let collection_circulation = circulating_supplies.get_mut(&ownership.collection);

                if let Some(circulation) = collection_circulation {
                    // Increment the circulating supply
                    *circulation += 1;
                } else {
                    circulating_supplies.insert(ownership.collection, 1);
                }

                // Verify that the token_id is within the max supply
                assert!(
                    ownership.token_id <= *data.collection_supplies.get(&ownership.collection).unwrap_or(&0),
                    "Token has too high token ID! - Collection: {:?}, Max supply {:?}, Token ID: {}, Owner: {:?}, Score: {}",
                    ownership.collection,
                    max_supply,
                    ownership.token_id,
                    ownership.owner,
                    ownership.mod_score
                );

                info!(
                    "Token state verification OK - Address: {:?}, collection: {:?}, tid: {}, balance: {:?}",
                    ownership.owner, ownership.collection, ownership.token_id, holdings
                );
            }

            // Collect all mintable collections
            let mintable_collections = AHashSet::from_iter(data.collections.iter().map(|c| c.tick));

            // Verify that all local mint counts match the remote mint counts
            for collection_supply in data.collection_supplies.iter() {
                let rtx = self.db.read_tx();

                // Retrieve the circulating supply for the collection
                let circulating_supply =
                    circulating_supplies.get(collection_supply.0).unwrap_or(&0);

                // Retrieve the last minted token for the collection from the database
                let last_mint = self
                    .db
                    .mint_history
                    .last_minted_token_seq_no_rtx(&rtx, collection_supply.0)?
                    .map(|t| t.seq_no)
                    .unwrap_or_default();

                // If this collection is mintable, we need to verify the mint count
                if mintable_collections.contains(collection_supply.0) {
                    // Retrieve the mintable collection
                    let mintable_collection = data
                        .collections
                        .iter()
                        .find(|c| c.tick == *collection_supply.0)
                        .unwrap();

                    // Verify that the circulating supply matches the mint count
                    assert_eq!(
                        mintable_collection.minted,
                        *circulating_supply,
                        "Circulating supply count doesn't match mint count - Collection: {:?}, Local mint count: {} Circulating supply: {}",
                        collection_supply.0,
                        mintable_collection.minted,
                        circulating_supply
                    );

                    // Verify that the mint count matches the last mint from the database
                    assert_eq!(
                        mintable_collection.minted,
                        last_mint,
                        "Collection mint count doesnt match - Collection: {:?}, Local mint count: {} Remote mint count: {}",
                        collection_supply.0,
                        mintable_collection.minted,
                        last_mint
                    );

                    info!(
                        "Collection mint count verification OK - Collection: {:?}",
                        collection_supply.0
                    );
                    continue;
                }

                // If this collection doesn't exist in mintable collections
                // it means its minted out. This means that the last mint should
                // equal the circulating supply as well as the max supply.

                // Previously we also checked for duplicates so this MUST mean
                // that ALL token ids have been minted out.

                // Verify that the circulating supply matches the remote mint count
                assert_eq!(
                    last_mint,
                    *circulating_supply,
                    "Circulating supply count doesn't match remote mint count - Collection: {:?}, Remote mint count: {} Circulating supply: {}", 
                    collection_supply.0,
                    last_mint,
                    circulating_supply
                );

                // Verify that the last mint matches the collection supply
                assert_eq!(
                    last_mint,
                    *collection_supply.1,
                    "Collection not minted out - Collection: {:?}, Remaining: {}",
                    collection_supply.0,
                    collection_supply.1 - last_mint
                );

                info!(
                    "Collection mint count verification OK - Collection: {:?}",
                    collection_supply.0
                );
            }

            info!("All token verifications completed successfully! :)");
            self.runtime.terminate();
        }

        self.shutdown.response.send(()).await?;
        Ok(())
    }
}

#[async_trait]
impl Service for ProcLoad {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        info!("starting procload service");
        let span = tracing::Span::current();
        task::spawn(
            async move {
                self.task()
                    .instrument(info_span!("PROCLOAD task"))
                    .await
                    .unwrap_or_else(|err| error!("{} error: {}", "PROCLOAD", err));
            }
            .instrument(span),
        );

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        self.shutdown.request.try_send(()).unwrap();
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        self.shutdown.response.recv().await?;
        Ok(())
    }
}
