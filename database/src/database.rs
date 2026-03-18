use crate::result::Result;
use fjall::compaction::{Fifo, Strategy};
use fjall::{
    Config, Keyspace, KvPair, KvSeparationOptions, PartitionCreateOptions, ReadTransaction, Slice,
    TxKeyspace, TxPartitionHandle, UserKey, UserValue,
};
use kaspa_consensus_core::tx::TransactionId;
use krc721_core::network::Network;
use std::fmt;
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, RwLock};
use tracing::info;

mod partition;
pub use partition::*;
mod stats;
pub use stats::*;

const REJECTIONS_SIZE_LIMIT: u64 = 1024 * 1024 * 1024; // 1 GB
const REJECTIONS_LEN_LIMIT: u64 = 1_000_000; // 1M

pub const PARTITION_BLOCKHASH_TO_SCORE: &str = "blockhash_to_score";
pub const PARTITION_CHAIN_BLOCK_SCORES: &str = "chain_block_scores";
pub const PARTITION_OPERATION_HISTORY: &str = "operation_history";
pub const PARTITION_TX_ID_TO_OPSCORE: &str = "tx_id_to_opscore";
pub const PARTITION_COLLECTION_DEPLOYMENTS: &str = "collection_deployments";
pub const PARTITION_TICK_CREATION_HISTORY: &str = "tick_creation_history";
pub const PARTITION_OWNERSHIP_CHANGES: &str = "ownership_changes";
pub const PARTITION_MINT_HISTORY: &str = "mint_history";
pub const PARTITION_OWNERSHIP_HISTORY: &str = "ownership_history";
pub const PARTITION_CURRENT_OWNERSHIP: &str = "current_ownership";
pub const PARTITION_AVAILABLE_RANGES: &str = "available_ranges";
pub const PARTITION_TOKEN_ID_META: &str = "token_id_meta";
pub const PARTITION_ADDRESS_HOLDINGS: &str = "address_holdings";
pub const PARTITION_RANGE_LENGTHS: &str = "range_lengths";
pub const PARTITION_DATABASE_STATS: &str = "database_stats";
pub const PARTITION_NOTIFICATION_QUEUE: &str = "notification_queue";
pub const PARTITION_SCORE_TO_DISCOUNT: &str = "score_to_discount";
pub const PARTITION_VIP: &str = "vip";
pub const PARTITION_REJECTIONS_BY_TXID: &str = "rejections_by_txid";

pub const PARTITION_TX_ID_TO_REJECTION: &str = "txid_to_rejection";

pub const PARTITION_SERIAL_TO_REJECTED_TXID: &str = "serial_to_rejected_txid";

pub const PARTITION_LISTINGS: &str = "listings";
pub const PARTITION_LISTINGS_BY_TICK: &str = "listings_by_tick";
pub const PARTITION_ADDRESS_LISTINGS: &str = "address_listings";

pub const DEFAULT_REJECTION_LEN: usize = 64;

pub struct Snapshot {
    pub keyspace: Keyspace,
    pub name: Arc<str>,
    pub snapshot: fjall::Snapshot,
}

#[derive(Clone)]
pub struct Db {
    /// The transaction keyspace for the database
    keyspace: TxKeyspace,

    /// Maps block hashes to their blue scores for efficient score lookups.
    /// Essential for chain reorganization processing and block validation.
    ///
    /// Storage format:
    /// - Key: BlockHash
    /// - Value: blue_score (u64)
    pub blockhash_to_score: BlockScorePartition,

    /// Maintains the canonical chain state by tracking accepted blocks and their blue scores.
    /// During chain reorganization, this partition is used to identify and process removed blocks.
    /// Blocks are ordered by blue score to facilitate efficient chain traversal and reorg handling.
    ///
    /// Storage format:
    /// - Key: {blue_score}:{BlockHash}
    /// - Value: ()
    ///
    /// Primary uses:
    /// - Tracking the current accepted chain state
    /// - Identifying blocks affected by reorgs
    /// - Maintaining block order for consistent processing
    pub chain_block_scores: AcceptedBlocksPartition,

    /// Chronicles all NFT operations in chronological order, maintaining their validation status.
    /// Each operation is indexed by its transaction score to preserve temporal ordering.
    /// This partition serves as the authoritative record of all NFT-related transactions.
    ///
    /// Storage format:
    /// - Key: {tx_score}
    /// - Value: {CheckedOperation}
    ///
    /// Primary uses:
    /// - Historical operation queries
    /// - Reorg handling and state reconstruction
    /// - Operation validation and verification
    pub operation_history: OperationByScorePartition,
    pub tx_id_to_opscore: OpScoreByTxIdPartition,

    /// Stores collection deployment history with full deployment metadata.
    /// Essential for collection validation and reorg processing.
    ///
    /// Storage format:
    /// - Key: {tx_score}:{tick}
    /// - Value: DeployInfoWithCommon
    ///
    /// Primary uses:
    /// - Deployment history tracking
    /// - Collection validation
    /// - Reorg processing
    pub collection_deployments: DeploymentHistoryPartition,

    /// Maps collection ticks to their initial deployment information.
    /// Enforces collection uniqueness and tracks deployment status.
    ///
    /// Storage format:
    /// - Key: {tick}
    /// - Value: ScoredDeployInfoWithCommon
    ///
    /// Primary uses:
    /// - Collection uniqueness enforcement
    /// - Deployment validation
    /// - Registry management
    pub collection_registry: CollectionRegistryPartition,

    /// Tracks token minting sequence by collection.
    /// Ensures proper token ID assignment and minting order.
    ///
    /// Storage format:
    /// - Key: {tick}:{reversed_seq_number}:{token_id}:{tx_score}
    /// - Value: ()
    ///
    /// Primary uses:
    /// - Minting sequence tracking
    /// - Token ID validation
    /// - Supply management
    pub mint_history: MintHistoryPartition,

    /// Chronicles token ownership changes in chronological order.
    /// Essential for ownership history and state reconstruction.
    ///
    /// Storage format:
    /// - Key: {tx_score}:{tick}:{token_id}:{reversed_seq_number}
    /// - Value: ()
    ///
    /// Primary uses:
    /// - Transfer history tracking
    /// - State reconstruction
    /// - Ownership validation
    pub ownership_changes: OwnershipChangesPartition,

    /// Maintains complete token ownership history in reverse chronological order.
    /// Crucial for ownership verification and state recovery.
    ///
    /// Storage format:
    /// - Key: {tick}:{token_id}:{reversed_tx_score}
    /// - Value: ScriptPublicKey
    ///
    /// Primary uses:
    /// - Historical ownership tracking
    /// - State reconstruction
    /// - Transfer validation
    pub ownership_history: OwnershipHistoryPartition,

    /// Stores current token ownership state for quick lookups.
    /// Provides efficient access to current token holders.
    ///
    /// Storage format:
    /// - Key: {tick}:{token_id}
    /// - Value: (mod_tx_score, ScriptPublicKey)
    ///
    /// Primary uses:
    /// - Current ownership queries
    /// - Transfer validation
    /// - State management
    pub current_ownership: CurrentOwnershipPartition,

    /// Stores available token ranges for each collection.
    /// Enables random token ID generation. Works similar to a
    /// list. Indexes are used to track available ranges.
    ///
    /// Storage format:
    /// - Key: {tick}:{index}
    /// - Value: (u64,u64) (start,size)
    ///
    /// Primary uses:
    /// - Token ID generation
    pub available_ranges: AvailableRangesPartition,

    /// Stores the number of available ranges for each collection
    /// Provides quick access to the count of ranges per collection.
    ///
    /// Storage format:
    /// - Key: {tick}
    /// - Value: Range count
    ///
    /// Primary uses:
    /// - Range counting
    /// - Availability tracking
    pub range_lengths: RangeLengthPartition,

    /// Contains information on how a particular token ID was generated.
    /// Tracks the original range that was modified in this token id generation.
    ///
    /// Storage format:
    /// - Key: {tick}:{token_id}
    /// - Value: original range data
    ///
    /// Primary uses:
    /// - Rollback token ID generation during reorgs
    pub token_id_meta: TokenIdMetaPartition,

    /// Maps addresses to their owned tokens.
    /// Enables efficient portfolio queries and balance tracking.
    ///
    /// Storage format:
    /// - Key: {script_public_key}:{tick}:{token_id}
    /// - Value: mod_tx_score
    ///
    /// Primary uses:
    /// - Portfolio queries
    /// - Balance tracking
    /// - Holdings validation
    pub address_holdings: AddressHoldingsPartition,

    /// Tracks database stats
    ///
    /// Storage format:
    /// - Key: {StatsKey} (u8)
    /// - Value: u64
    ///
    /// Primary uses:
    /// - Obtaining statistics of the indexer operations
    pub stats: StatsPartition,

    /// Maintains queue of pending chain updates for processing.
    /// Enables ordered processing of chain reorganizations.
    ///
    /// Storage format:
    /// - Key: {sequence_number}
    /// - Value: VirtualChainChanges
    ///
    /// Primary uses:
    /// - Chain update queueing
    /// - Reorg coordination
    /// - State synchronization
    pub notification_queue: NotificationQueuePartition,

    /// Tracks discounts granted by collection deployers indexed by transaction score.
    /// Maps transaction scores to discount information for efficient lookup and reorg handling.
    ///
    /// Storage format:
    /// - Key: {tx_score}:{tick}:{script_public_key}
    /// - Value: discount_fee (u64)
    ///
    /// Primary uses:
    /// - Discount validation
    /// - Fee calculation
    /// - Reorg processing
    pub score_to_discount: ScoredDiscountsPartition,

    /// Stores VIP discount information ordered by reversed transaction score.
    /// Maintains discount history for addresses with special pricing privileges.
    ///
    /// Storage format:
    /// - Key: {script_public_key}:{tick}:{reversed_tx_score}:{fee}
    /// - Value: ()
    ///
    /// Primary uses:
    /// - VIP fee lookups
    /// - Discount history tracking
    /// - Privilege management
    pub vip: VipPartition,
    pub tx_id_to_rejection: TxIDToRejectionPartition,
    pub serial_to_rejected_tx_id: SerialToRejectedTxIDPartition,

    /// Active NFT marketplace listings. Key: {tick}:{token_id}
    pub listings: ListingsPartition,
    /// Listings sorted by price per collection. Key: {tick}:{price}:{token_id}
    pub listings_by_tick: ListingsByTickPartition,
    /// Seller's active listings. Key: {spk}:{tick}:{token_id}
    pub address_listings: AddressListingsPartition,
    snapshot_commit_rw: Arc<RwLock<()>>,
    serial_rejection: Arc<AtomicU64>,
}

impl Db {
    pub fn database_folder<P: AsRef<Path>>(data_dir: P, network: &Network) -> PathBuf {
        data_dir.as_ref().join(network.to_string())
    }

    pub fn write_tx(&self) -> WriteTransaction {
        let inner_tx = self.keyspace.write_tx().expect("Mutex lock poisoned");
        WriteTransaction {
            tx: inner_tx,
            commit_locker: self.snapshot_commit_rw.clone(),
        }
    }

    pub fn read_tx(&self) -> ReadTransaction {
        self.keyspace.read_tx()
    }

    pub fn try_open<P: AsRef<Path>>(data_dir: P, network: &Network) -> Result<Self> {
        let root = Self::database_folder(data_dir, network);

        std::fs::create_dir_all(&root)?;

        let keyspace = Config::new(root).open_transactional()?;
        let blockhash_to_score = Partition::new(keyspace.open_partition(
            PARTITION_BLOCKHASH_TO_SCORE,
            PartitionCreateOptions::default(),
        )?);
        let chain_block_scores = Partition::new(keyspace.open_partition(
            PARTITION_CHAIN_BLOCK_SCORES,
            PartitionCreateOptions::default(),
        )?);
        let operation_history = Partition::new(keyspace.open_partition(
            PARTITION_OPERATION_HISTORY,
            PartitionCreateOptions::default(),
        )?);
        let tx_id_to_opscore = Partition::new(keyspace.open_partition(
            PARTITION_TX_ID_TO_OPSCORE,
            PartitionCreateOptions::default(),
        )?);

        let collection_deployments = Partition::new(keyspace.open_partition(
            PARTITION_COLLECTION_DEPLOYMENTS,
            PartitionCreateOptions::default(),
        )?);
        let tick_creation_history = Partition::new(keyspace.open_partition(
            PARTITION_TICK_CREATION_HISTORY,
            PartitionCreateOptions::default(),
        )?);
        let ownership_changes = Partition::new(keyspace.open_partition(
            PARTITION_OWNERSHIP_CHANGES,
            PartitionCreateOptions::default(),
        )?);
        let mint_history = Partition::new(
            keyspace.open_partition(PARTITION_MINT_HISTORY, PartitionCreateOptions::default())?,
        );

        let ownership_history = Partition::new(keyspace.open_partition(
            PARTITION_OWNERSHIP_HISTORY,
            PartitionCreateOptions::default(),
        )?);

        let current_ownership = Partition::new(keyspace.open_partition(
            PARTITION_CURRENT_OWNERSHIP,
            PartitionCreateOptions::default(),
        )?);
        let available_ranges = Partition::new(keyspace.open_partition(
            PARTITION_AVAILABLE_RANGES,
            PartitionCreateOptions::default(),
        )?);

        let token_id_meta = Partition::new(
            keyspace.open_partition(PARTITION_TOKEN_ID_META, PartitionCreateOptions::default())?,
        );

        let address_holdings = Partition::new(keyspace.open_partition(
            PARTITION_ADDRESS_HOLDINGS,
            PartitionCreateOptions::default(),
        )?);

        let range_lengths = Partition::new(
            keyspace.open_partition(PARTITION_RANGE_LENGTHS, PartitionCreateOptions::default())?,
        );

        let stats = Partition::new(
            keyspace.open_partition(PARTITION_DATABASE_STATS, PartitionCreateOptions::default())?,
        );

        let notification_queue = Partition::new({
            let opts = PartitionCreateOptions::default()
                .with_kv_separation(KvSeparationOptions::default()); // todo calculate separation opts

            // todo we dont have logic to verify that fifo was not applied, so we can't use that
            // .compaction_strategy(Strategy::Fifo(
            //     Fifo{ limit: /*1 GB*/ 1024*1024*1024, ttl_seconds: None }, // todo properly calculate
            // ));
            keyspace.open_partition(PARTITION_NOTIFICATION_QUEUE, opts)?
        });
        // purge queue
        {
            let mut tx = keyspace.write_tx()?;
            notification_queue.purge(&mut tx)?;
            tx.commit()?.expect("conflict is unexpected");
        }

        let score_to_discount = Partition::new(keyspace.open_partition(
            PARTITION_SCORE_TO_DISCOUNT,
            PartitionCreateOptions::default(),
        )?);

        let vip = Partition::new(
            keyspace.open_partition(PARTITION_VIP, PartitionCreateOptions::default())?,
        );

        let tx_id_to_rejection = Partition::new(keyspace.open_partition(
            PARTITION_TX_ID_TO_REJECTION,
            PartitionCreateOptions::default(),
        )?);
        let serial_to_rejected_txid = Partition::new(keyspace.open_partition(
            PARTITION_SERIAL_TO_REJECTED_TXID,
            PartitionCreateOptions::default(),
        )?);

        let listings = Partition::new(
            keyspace.open_partition(PARTITION_LISTINGS, PartitionCreateOptions::default())?,
        );
        let listings_by_tick = Partition::new(
            keyspace.open_partition(PARTITION_LISTINGS_BY_TICK, PartitionCreateOptions::default())?,
        );
        let address_listings = Partition::new(keyspace.open_partition(
            PARTITION_ADDRESS_LISTINGS,
            PartitionCreateOptions::default(),
        )?);
        let serial = if keyspace
            .list_partitions()
            .iter()
            .any(|p| p.as_ref() == PARTITION_REJECTIONS_BY_TXID)
        {
            info!("rejection partition found, start migration");
            let rejections_by_txid =
                Partition::new(keyspace.open_partition(
                    PARTITION_REJECTIONS_BY_TXID,
                    PartitionCreateOptions::default().compaction_strategy(Strategy::Fifo(
                        Fifo::new(REJECTIONS_SIZE_LIMIT, None),
                    )),
                )?);

            let s = Self::migrate(
                &keyspace,
                &rejections_by_txid,
                &serial_to_rejected_txid,
                &tx_id_to_rejection,
            )?;
            keyspace.delete_partition(rejections_by_txid.partition)?;
            s
        } else {
            serial_to_rejected_txid.last_key()?.unwrap_or_default() + 1
        };

        Ok(Self {
            range_lengths,
            token_id_meta,
            keyspace,
            blockhash_to_score,
            chain_block_scores,
            operation_history,
            tx_id_to_opscore,
            collection_deployments,
            collection_registry: tick_creation_history,
            mint_history,
            ownership_changes,
            ownership_history,
            current_ownership,
            available_ranges,
            address_holdings,
            stats,
            notification_queue,
            score_to_discount,
            vip,
            tx_id_to_rejection,
            snapshot_commit_rw: Arc::new(Default::default()),
            serial_to_rejected_tx_id: serial_to_rejected_txid,
            listings,
            listings_by_tick,
            address_listings,
            serial_rejection: Arc::new(serial.into()),
        })
    }

    pub fn migrate(
        keyspace: &TxKeyspace,
        rp: &RejectionPartition,
        serial_to_rejected_tx_idpartition: &SerialToRejectedTxIDPartition,
        tx_idto_rejection_partition: &TxIDToRejectionPartition,
    ) -> Result<u64> {
        let rtx = keyspace.read_tx();
        let mut serial = serial_to_rejected_tx_idpartition // useful if the app is stopped during migration
            .last_key()?
            .unwrap_or_default();
        for r in rp.range_rtx(&rtx, ..) {
            let (txid, rejection) = r?;
            serial_to_rejected_tx_idpartition.insert(serial, &txid)?;
            tx_idto_rejection_partition.insert(txid, &rejection)?;
            serial += 1;
        }
        Ok(serial)
    }

    pub fn reject_tx(
        &self,
        wtx: &mut WriteTransaction,
        tx_id: TransactionId,
        reason: &String,
    ) -> Result<()> {
        let serial = self.serial_rejection.fetch_add(1, SeqCst);
        if serial >= REJECTIONS_LEN_LIMIT {
            let serial_to_remove = serial - REJECTIONS_LEN_LIMIT;
            if let Some(txid) = self
                .serial_to_rejected_tx_id
                .get_wtx(wtx, &serial_to_remove)?
            {
                self.serial_to_rejected_tx_id
                    .remove_rejection(wtx, serial_to_remove)?;
                self.tx_id_to_rejection.remove_rejection(wtx, txid)?;
            }
        }
        self.tx_id_to_rejection
            .insert_rejection(wtx, tx_id, reason)?;
        self.serial_to_rejected_tx_id
            .insert_rejection(wtx, serial, tx_id)?;
        Ok(())
    }

    pub fn disk_space(&self) -> u64 {
        self.keyspace.disk_space()
    }

    pub fn take_snapshots(&self) -> [Snapshot; 20] {
        let keyspace = || self.keyspace.inner().clone();
        let _guard = self.snapshot_commit_rw.write().unwrap();
        let seq_no = self.keyspace.inner().instant();

        [
            self.blockhash_to_score.snapshot_at(keyspace(), seq_no),
            self.chain_block_scores.snapshot_at(keyspace(), seq_no),
            self.operation_history.snapshot_at(keyspace(), seq_no),
            self.tx_id_to_opscore.snapshot_at(keyspace(), seq_no),
            self.collection_deployments.snapshot_at(keyspace(), seq_no),
            self.collection_registry.snapshot_at(keyspace(), seq_no),
            self.mint_history.snapshot_at(keyspace(), seq_no),
            self.ownership_changes.snapshot_at(keyspace(), seq_no),
            self.ownership_history.snapshot_at(keyspace(), seq_no),
            self.current_ownership.snapshot_at(keyspace(), seq_no),
            self.available_ranges.snapshot_at(keyspace(), seq_no),
            self.range_lengths.snapshot_at(keyspace(), seq_no),
            self.token_id_meta.snapshot_at(keyspace(), seq_no),
            self.address_holdings.snapshot_at(keyspace(), seq_no),
            self.stats.snapshot_at(keyspace(), seq_no),
            self.score_to_discount.snapshot_at(keyspace(), seq_no),
            self.vip.snapshot_at(keyspace(), seq_no),
            // todo are they needed??
            self.notification_queue.snapshot_at(keyspace(), seq_no),
            self.tx_id_to_rejection.snapshot_at(keyspace(), seq_no),
            self.serial_to_rejected_tx_id
                .snapshot_at(keyspace(), seq_no),
        ]
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        tracing::info!("Dropping Database...");
    }
}

pub trait Key {
    type OwnedKey: AsRef<[u8]>;
    fn owned_key(&self) -> Self::OwnedKey;
    fn key(&self) -> Slice {
        Slice::new(self.owned_key().as_ref())
    }
    fn from_key_bytes(key_bytes: &[u8]) -> Self;
}

pub struct WriteTransaction {
    tx: fjall::WriteTransaction,
    commit_locker: Arc<RwLock<()>>,
}

impl WriteTransaction {
    #[inline]
    pub fn get<K: AsRef<[u8]>>(
        &mut self,
        partition: &TxPartitionHandle,
        key: K,
    ) -> fjall::Result<Option<UserValue>> {
        self.tx.get(partition, key)
    }

    #[inline]
    pub fn range<'b, K: AsRef<[u8]> + 'b, R: RangeBounds<K> + 'b>(
        &'b mut self,
        partition: &'b TxPartitionHandle,
        range: R,
    ) -> impl DoubleEndedIterator<Item = fjall::Result<KvPair>> + 'b {
        self.tx.range(partition, range)
    }

    #[inline]
    pub fn insert<K: Into<UserKey>, V: Into<UserValue>>(
        &mut self,
        partition: &TxPartitionHandle,
        key: K,
        value: V,
    ) {
        self.tx.insert(partition, key, value)
    }

    #[inline]
    pub fn remove<K: Into<UserKey>>(&mut self, partition: &TxPartitionHandle, key: K) {
        self.tx.remove(partition, key)
    }
    #[inline]
    pub fn fetch_update<K: Into<UserKey>, F: FnMut(Option<&UserValue>) -> Option<UserValue>>(
        &mut self,
        partition: &TxPartitionHandle,
        key: K,
        f: F,
    ) -> fjall::Result<Option<UserValue>> {
        self.tx.fetch_update(partition, key, f)
    }

    #[inline]
    pub fn contains_key<K: AsRef<[u8]>>(
        &mut self,
        partition: &TxPartitionHandle,
        key: K,
    ) -> fjall::Result<bool> {
        self.tx.contains_key(partition, key)
    }

    #[inline]
    pub fn last_key_value(
        &mut self,
        partition: &TxPartitionHandle,
    ) -> fjall::Result<Option<KvPair>> {
        self.tx.last_key_value(partition)
    }

    #[inline]
    pub fn first_key_value(
        &mut self,
        partition: &TxPartitionHandle,
    ) -> fjall::Result<Option<KvPair>> {
        self.tx.first_key_value(partition)
    }
    #[inline]
    pub fn prefix<'b, K: AsRef<[u8]> + 'b>(
        &'b mut self,
        partition: &'b TxPartitionHandle,
        prefix: K,
    ) -> impl DoubleEndedIterator<Item = fjall::Result<KvPair>> + 'b {
        self.tx.prefix(partition, prefix)
    }

    pub fn commit(self) -> fjall::Result<Result<(), Conflict>> {
        let _guard = self.commit_locker.read().unwrap();
        self.tx.commit().map(|r| r.map_err(|_| Conflict))
    }
}

#[derive(Debug)]
pub struct Conflict;

impl std::error::Error for Conflict {}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        "transaction conflict".fmt(f)
    }
}
