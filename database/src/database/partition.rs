use crate::database::{Key, Snapshot, WriteTransaction};
use crate::database::{Stats, StatsDiffs};
use crate::error::Error;
use borsh::{BorshDeserialize, BorshSerialize};
use bytes::{BufMut, BytesMut};
use fjall::{Keyspace, ReadTransaction, TxPartition, UserValue};
use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_core::{tx::ScriptPublicKey, Hash};
use krc721_core::model::krc721::database::*;
use krc721_core::model::krc721::tick::*;
use smallvec::SmallVec;
use std::any::type_name;
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::ops::{Bound, RangeBounds};

impl Key for Tick {
    type OwnedKey = [u8; TICK_LENGTH];

    fn owned_key(&self) -> Self::OwnedKey {
        self.0
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        Self(<[u8; TICK_LENGTH]>::try_from(key_bytes).unwrap())
    }
}

pub struct Partition<K: Key, V: BorshDeserialize + BorshDeserialize> {
    pub partition: TxPartition,
    pub key: PhantomData<K>,
    pub value: PhantomData<V>,
}

impl<K: Key, V: BorshDeserialize + BorshDeserialize> Partition<K, V> {
    pub fn new(partition: TxPartition) -> Self {
        Self {
            partition,
            key: Default::default(),
            value: Default::default(),
        }
    }

    pub fn snapshot_at(&self, keyspace: Keyspace, instant: fjall::Instant) -> Snapshot {
        let inner = self.partition.inner();
        Snapshot {
            keyspace,
            name: inner.name.clone(),
            snapshot: inner.snapshot_at(instant),
        }
    }
}

impl<K: Key, V: BorshDeserialize + BorshDeserialize> Clone for Partition<K, V> {
    fn clone(&self) -> Self {
        Self {
            partition: self.partition.clone(),
            key: Default::default(),
            value: Default::default(),
        }
    }
}

impl<K: Key, V: BorshDeserialize + BorshSerialize> Partition<K, V> {
    pub fn insert(&self, k: K, v: &V) -> Result<(), Error> {
        let v =
            borsh::to_vec(v).map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))?;
        self.partition.insert(k.key(), v)?;
        Ok(())
    }

    pub fn insert_wtx(&self, tx: &mut WriteTransaction, k: K, v: &V) -> Result<(), Error> {
        let v =
            borsh::to_vec(v).map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))?;
        tx.insert(&self.partition, k.key(), v);
        Ok(())
    }

    pub fn insert_if_not_exist_wtx(
        &self,
        tx: &mut WriteTransaction,
        k: K,
        v: &V,
    ) -> Result<bool, Error> {
        let v = {
            let bytes = BytesMut::new(); // todo capacity
            let mut writer = bytes.writer();
            borsh::to_writer(&mut writer, v)
                .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))?;
            writer.into_inner().freeze()
        };

        let mut inserted = false;
        tx.fetch_update(&self.partition, k.key(), |current| match current {
            None => {
                inserted = true;
                Some(UserValue::from(v.clone()))
            }
            Some(v) => Some(v.clone()),
        })?;
        Ok(inserted)
    }

    pub fn get(&self, k: &K) -> Result<Option<V>, Error> {
        self.partition
            .get(k.key())?
            .as_deref()
            .map(borsh::from_slice)
            .transpose()
            .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))
    }

    pub fn get_rtx(&self, tx: &ReadTransaction, k: &K) -> Result<Option<V>, Error> {
        tx.get(&self.partition, k.key())?
            .as_deref()
            .map(borsh::from_slice)
            .transpose()
            .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))
    }

    pub fn get_wtx(&self, tx: &mut WriteTransaction, k: &K) -> Result<Option<V>, Error> {
        tx.get(&self.partition, k.key())?
            .as_deref()
            .map(borsh::from_slice)
            .transpose()
            .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))
    }

    pub fn contains_key_wtx(&self, tx: &mut WriteTransaction, k: &K) -> Result<bool, Error> {
        Ok(tx.contains_key(&self.partition, k.key())?)
    }

    pub fn remove(&self, k: &K) -> Result<(), Error> {
        Ok(self.partition.remove(k.key())?)
    }

    pub fn remove_wtx(&self, tx: &mut WriteTransaction, k: &K) -> Result<(), Error> {
        tx.remove(&self.partition, k.key());
        Ok(())
    }

    /// Removes the value at the specified key if it exists and returns its value.
    pub fn remove_if_exists_wtx(
        &self,
        tx: &mut WriteTransaction,
        k: &K,
    ) -> Result<Option<V>, Error> {
        tx.fetch_update(&self.partition, k.key(), |_| None)?
            .map(|v| borsh::from_slice(v.as_ref()))
            .transpose()
            .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))
    }

    pub fn range_rtx<R: RangeBounds<K>>(
        &self,
        tx: &ReadTransaction,
        range: R,
    ) -> impl DoubleEndedIterator<Item = Result<(K, V), Error>> {
        // Convert to owned byte vectors to satisfy 'static bound
        let start_bound = match range.start_bound() {
            Bound::Included(k) => Bound::Included(k.key()),
            Bound::Excluded(k) => Bound::Excluded(k.key()),
            Bound::Unbounded => Bound::Unbounded,
        };

        let end_bound = match range.end_bound() {
            Bound::Included(k) => Bound::Included(k.key()),
            Bound::Excluded(k) => Bound::Excluded(k.key()),
            Bound::Unbounded => Bound::Unbounded,
        };

        tx.range(&self.partition, (start_bound, end_bound))
            .map(|r| {
                r.map_err(Into::into).and_then(|(k, v)| {
                    let k = K::from_key_bytes(k.as_ref());
                    let v: V = borsh::from_slice(v.as_ref())
                        .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))?;
                    Ok((k, v))
                })
            })
    }

    pub fn range_wtx<'a: 'b, 'b, R: RangeBounds<K>>(
        &'a self,
        tx: &'b mut WriteTransaction,
        range: R,
    ) -> impl DoubleEndedIterator<Item = Result<(K, V), Error>> + 'b {
        // Convert to owned byte vectors to satisfy 'static bound
        let start_bound = match range.start_bound() {
            Bound::Included(k) => Bound::Included(k.key()),
            Bound::Excluded(k) => Bound::Excluded(k.key()),
            Bound::Unbounded => Bound::Unbounded,
        };

        let end_bound = match range.end_bound() {
            Bound::Included(k) => Bound::Included(k.key()),
            Bound::Excluded(k) => Bound::Excluded(k.key()),
            Bound::Unbounded => Bound::Unbounded,
        };

        tx.range(&self.partition, (start_bound, end_bound))
            .map(move |r| {
                r.map_err(Into::into).and_then(|(k, v)| {
                    let k = K::from_key_bytes(k.as_ref());
                    let v: V = borsh::from_slice(v.as_ref())
                        .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))?;
                    Ok((k, v))
                })
            })
    }

    pub fn range_keys_wtx<'a: 'b, 'b, R: RangeBounds<K>>(
        &'a self,
        tx: &'b mut WriteTransaction,
        range: R,
    ) -> impl DoubleEndedIterator<Item = Result<K, Error>> + 'b {
        // Convert to owned byte vectors to satisfy 'static bound
        let start_bound = match range.start_bound() {
            Bound::Included(k) => Bound::Included(k.key()),
            Bound::Excluded(k) => Bound::Excluded(k.key()),
            Bound::Unbounded => Bound::Unbounded,
        };

        let end_bound = match range.end_bound() {
            Bound::Included(k) => Bound::Included(k.key()),
            Bound::Excluded(k) => Bound::Excluded(k.key()),
            Bound::Unbounded => Bound::Unbounded,
        };

        tx.range(&self.partition, (start_bound, end_bound))
            .map(move |r| {
                r.map_err(Into::into).map(|(k, _v)| {
                    let k = K::from_key_bytes(k.as_ref());
                    k
                })
            })
    }

    pub fn range_keys_rtx<'a: 'b, 'b, R: RangeBounds<K>>(
        &'a self,
        tx: &'b ReadTransaction,
        range: R,
    ) -> impl DoubleEndedIterator<Item = Result<K, Error>> + 'b {
        // Convert to owned byte vectors to satisfy 'static bound
        let start_bound = match range.start_bound() {
            Bound::Included(k) => Bound::Included(k.key()),
            Bound::Excluded(k) => Bound::Excluded(k.key()),
            Bound::Unbounded => Bound::Unbounded,
        };

        let end_bound = match range.end_bound() {
            Bound::Included(k) => Bound::Included(k.key()),
            Bound::Excluded(k) => Bound::Excluded(k.key()),
            Bound::Unbounded => Bound::Unbounded,
        };

        tx.range(&self.partition, (start_bound, end_bound))
            .map(move |r| {
                r.map_err(Into::into).map(|(k, _v)| {
                    let k = K::from_key_bytes(k.as_ref());
                    k
                })
            })
    }

    pub fn last_key_wtx(&self, tx: &mut WriteTransaction) -> Result<Option<K>, Error> {
        Ok(tx.last_key_value(&self.partition)?.map(|(k, _v)| {
            let k = K::from_key_bytes(k.as_ref());
            k
        }))
    }

    pub fn last_key(&self) -> Result<Option<K>, Error> {
        Ok(self.partition.last_key_value()?.map(|(k, _v)| {
            let k = K::from_key_bytes(k.as_ref());
            k
        }))
    }

    pub fn first_key_wtx(&self, tx: &mut WriteTransaction) -> Result<Option<K>, Error> {
        Ok(tx.first_key_value(&self.partition)?.map(|(k, _v)| {
            let k = K::from_key_bytes(k.as_ref());
            k
        }))
    }
}

type BlockHash = Hash;
impl Key for u64 {
    type OwnedKey = [u8; size_of::<u64>()];

    fn owned_key(&self) -> Self::OwnedKey {
        self.to_be_bytes()
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        Self::from_be_bytes(key_bytes.try_into().unwrap())
    }
}

pub type BlockScorePartition = Partition<BlockHash, u64>;

impl Key for BlockHash {
    type OwnedKey = [u8; size_of::<BlockHash>()];

    fn owned_key(&self) -> Self::OwnedKey {
        self.as_bytes()
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        Self::from_bytes(key_bytes.try_into().unwrap())
    }
}

impl Key for BlueScoredChainBlockHash {
    type OwnedKey = [u8; size_of::<u64>() + size_of::<BlockHash>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key = [0u8; size_of::<u64>() + size_of::<BlockHash>()]; // todo uninit
        key[..size_of::<u64>()].copy_from_slice(&self.blue_score.to_be_bytes());
        key[size_of::<u64>()..].copy_from_slice(&self.block_hash.as_bytes());
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (score, hash) = key_bytes.split_at(size_of::<u64>());
        let blue_score = u64::from_be_bytes(score.try_into().unwrap());
        let block_hash = BlockHash::from_slice(hash);
        Self {
            blue_score,
            block_hash,
        }
    }
}

pub type AcceptedBlocksPartition = Partition<BlueScoredChainBlockHash, ()>;

impl AcceptedBlocksPartition {
    pub fn last_accepted_block_rtx(
        &self,
        tx: &ReadTransaction,
    ) -> Result<Option<BlueScoredChainBlockHash>, Error> {
        Ok(tx
            .last_key_value(&self.partition)?
            .map(|(k, _v)| BlueScoredChainBlockHash::from_key_bytes(k.as_ref())))
    }

    pub fn last_accepted_block_wtx(
        &self,
        tx: &mut WriteTransaction,
    ) -> Result<Option<BlueScoredChainBlockHash>, Error> {
        Ok(tx
            .last_key_value(&self.partition)?
            .map(|(k, _v)| BlueScoredChainBlockHash::from_key_bytes(k.as_ref())))
    }
}

pub type OperationByScorePartition = Partition<u64, CheckedOperation>;

pub type OpScoreByTxIdPartition = Partition<TransactionId, u64>;

#[derive(Copy, Clone, Debug)]
pub struct DeploymentKey {
    pub score: u64,
    pub tick: Tick,
}

pub type DeploymentHistoryPartition = Partition<DeploymentKey, DeployInfoWithCommon>;

impl Key for DeploymentKey {
    type OwnedKey = [u8; size_of::<u64>() + size_of::<Tick>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key = [0u8; size_of::<u64>() + size_of::<Tick>()]; // todo uninit
        key[..size_of::<u64>()].copy_from_slice(&self.score.to_be_bytes());
        key[size_of::<u64>()..].copy_from_slice(&self.tick);
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let score = u64::from_be_bytes(key_bytes[..size_of::<u64>()].try_into().unwrap());
        let tick =
            unsafe { Tick::new_unchecked(key_bytes[size_of::<u64>()..].try_into().unwrap()) };
        DeploymentKey { score, tick }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TokenMintsKey {
    pub score: u64,
    pub tick: Tick,
    pub token_id: u64, // todo nonzerou64
    pub reversed_seq_number: u64,
}

impl TokenMintsKey {
    pub fn new(score: u64, tick: Tick, token_id: u64, reversed_seq_number: u64) -> Self {
        Self {
            score,
            tick,
            token_id,
            reversed_seq_number,
        }
    }
    pub fn with_seq(score: u64, tick: Tick, token_id: u64, seq: u64) -> Self {
        Self::new(score, tick, token_id, u64::MAX - seq)
    }
}

impl Key for TokenMintsKey {
    type OwnedKey =
        [u8; size_of::<u64>() + size_of::<Tick>() + size_of::<u64>() + size_of::<u64>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key =
            [0u8; size_of::<u64>() + size_of::<Tick>() + size_of::<u64>() + size_of::<u64>()]; // todo uninit
        let (score, remaining) = key.split_at_mut(size_of::<u64>());
        let (tick, remaining) = remaining.split_at_mut(size_of::<Tick>());
        let (token_id, reversed_seq_number) = remaining.split_at_mut(size_of::<u64>());
        score.copy_from_slice(&self.score.to_be_bytes());
        tick.copy_from_slice(&self.tick);
        token_id.copy_from_slice(&self.token_id.to_be_bytes());
        reversed_seq_number.copy_from_slice(&self.reversed_seq_number.to_be_bytes());
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (score, remaining) = key_bytes.split_at(size_of::<u64>());
        let score = u64::from_be_bytes(score.try_into().unwrap());
        let (tick, remaining) = remaining.split_at(size_of::<Tick>());
        let (token_id, reversed_seq_number) = remaining.split_at(size_of::<u64>());

        let tick = unsafe { Tick::new_unchecked(tick.try_into().unwrap()) };
        let token_id = u64::from_be_bytes(token_id.try_into().unwrap());
        let reversed_seq_number = u64::from_be_bytes(reversed_seq_number.try_into().unwrap());
        Self {
            score,
            tick,
            token_id,
            reversed_seq_number,
        }
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize, Default)]
pub struct ScoredDeployInfoWithCommon {
    pub score: u64,
    pub info: DeployInfoWithCommon,
}

pub type CollectionRegistryPartition = Partition<Tick, ScoredDeployInfoWithCommon>;

pub struct MintHistoryKey {
    pub tick: Tick,
    reversed_seq: u64,
    pub token_id: u64,
    pub score: u64,
}

impl MintHistoryKey {
    pub fn new(tick: Tick, reversed_seq: u64, token_id: u64, score: u64) -> Self {
        Self {
            reversed_seq,
            tick,
            token_id,
            score,
        }
    }

    pub fn with_seq(tick: Tick, seq: u64, token_id: u64, score: u64) -> Self {
        Self::new(tick, u64::MAX - seq, token_id, score)
    }
}

impl Key for MintHistoryKey {
    type OwnedKey =
        [u8; size_of::<Tick>() + size_of::<u64>() + size_of::<u64>() + size_of::<u64>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key =
            [0u8; size_of::<u64>() + size_of::<Tick>() + size_of::<u64>() + size_of::<u64>()]; // todo uninit
        let (tick, remaining) = key.split_at_mut(size_of::<Tick>());
        let (reversed_seq, remaining) = remaining.split_at_mut(size_of::<u64>());
        let (token_id, score) = remaining.split_at_mut(size_of::<u64>());

        tick.copy_from_slice(&self.tick);
        reversed_seq.copy_from_slice(&self.reversed_seq.to_be_bytes());
        token_id.copy_from_slice(&self.token_id.to_be_bytes());
        score.copy_from_slice(&self.score.to_be_bytes());
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (tick, remaining) = key_bytes.split_at(size_of::<Tick>());
        let tick = unsafe { Tick::new_unchecked(tick.try_into().unwrap()) };
        let (reversed_seq, remaining) = remaining.split_at(size_of::<u64>());
        let (token_id, score) = remaining.split_at(size_of::<u64>());
        let reversed_seq = u64::from_be_bytes(reversed_seq.try_into().unwrap());
        let token_id = u64::from_be_bytes(token_id.try_into().unwrap());
        let score = u64::from_be_bytes(score.try_into().unwrap());
        Self {
            reversed_seq,
            tick,
            token_id,
            score,
        }
    }
}

pub type MintHistoryPartition = Partition<MintHistoryKey, ()>;

impl MintHistoryPartition {
    pub fn last_minted_token_seq_no_rtx(
        &self,
        tx: &ReadTransaction,
        tick: &Tick,
    ) -> Result<Option<TokenIdScoreSeqNo>, Error> {
        Ok(tx
            .prefix(&self.partition, tick.0)
            .next()
            .transpose()
            .map(|opt_kv| {
                opt_kv.map(|kv| {
                    let k = MintHistoryKey::from_key_bytes(&kv.0);
                    TokenIdScoreSeqNo {
                        token_id: k.token_id,
                        score: k.score,
                        seq_no: u64::MAX - k.reversed_seq,
                    }
                })
            })?)
    }

    pub fn last_minted_token_seq_no_wtx(
        &self,
        tx: &mut WriteTransaction,
        tick: &Tick,
    ) -> Result<Option<TokenIdScoreSeqNo>, Error> {
        Ok(tx
            .prefix(&self.partition, tick.0)
            .next()
            .transpose()
            .map(|opt_kv| {
                opt_kv.map(|kv| {
                    let k = MintHistoryKey::from_key_bytes(&kv.0);
                    TokenIdScoreSeqNo {
                        token_id: k.token_id,
                        score: k.score,
                        seq_no: u64::MAX - k.reversed_seq,
                    }
                })
            })?)
    }
}

pub type OwnershipChangesPartition = Partition<TokenMintsKey, ()>;

pub struct OwnershipHistoryKey {
    pub tick: Tick,
    pub token_id: u64,
    reversed_score: u64,
}

impl OwnershipHistoryKey {
    pub fn new(tick: Tick, token_id: u64, reversed_score: u64) -> Self {
        Self {
            tick,
            token_id,
            reversed_score,
        }
    }
    pub fn with_score(tick: Tick, token_id: u64, score: u64) -> Self {
        Self {
            tick,
            token_id,
            reversed_score: u64::MAX - score,
        }
    }
    pub fn score(&self) -> u64 {
        u64::MAX - self.reversed_score
    }
}

impl Key for OwnershipHistoryKey {
    type OwnedKey = [u8; size_of::<Tick>() + size_of::<u64>() + size_of::<u64>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key = [0u8; size_of::<Tick>() + size_of::<u64>() + size_of::<u64>()]; // todo uninit
        let (tick, remaining) = key.split_at_mut(size_of::<Tick>());
        let (token_id, reversed_score) = remaining.split_at_mut(size_of::<u64>());
        tick.copy_from_slice(&self.tick);
        token_id.copy_from_slice(&self.token_id.to_be_bytes());
        reversed_score.copy_from_slice(&self.reversed_score.to_be_bytes());
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (tick, remaining) = key_bytes.split_at(size_of::<Tick>());
        let tick = unsafe { Tick::new_unchecked(tick.try_into().unwrap()) };
        let (token_id, reversed_score) = remaining.split_at(size_of::<u64>());
        let token_id = u64::from_be_bytes(token_id.try_into().unwrap());
        let reversed_score = u64::from_be_bytes(reversed_score.try_into().unwrap());
        Self {
            tick,
            token_id,
            reversed_score,
        }
    }
}

pub type OwnershipHistoryPartition = Partition<OwnershipHistoryKey, ScriptPublicKey>;

#[derive(Clone, Copy, Debug)]
pub struct TokenIdScoreSeqNo {
    pub token_id: u64,
    pub score: u64,
    pub seq_no: u64,
}
impl OwnershipHistoryPartition {
    pub fn last_owner_with_tx_mod_score_wtx(
        &self,
        tx: &mut WriteTransaction,
        key: &OwnershipKey,
    ) -> Result<Option<(ScriptPublicKey, ModTxScore)>, Error> {
        tx.prefix(&self.partition, key.key())
            .next()
            .map(|opt| {
                opt.map_err(Error::from).and_then(|(k, v)| {
                    let k = OwnershipHistoryKey::from_key_bytes(k.as_ref());
                    borsh::from_slice::<ScriptPublicKey>(v.as_ref())
                        .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))
                        .map(|v| (v, u64::MAX - k.reversed_score))
                })
            })
            .transpose()
    }
}

#[derive(Copy, Clone, Debug)]
pub struct OwnershipKey {
    pub tick: Tick,
    pub token_id: u64, // todo nonzerou64
}

impl Ord for OwnershipKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_key = self.key();
        let other_key = other.key();
        self_key.as_ref().cmp(other_key.as_ref())
    }
}

impl PartialOrd for OwnershipKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for OwnershipKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for OwnershipKey {}

impl Key for OwnershipKey {
    type OwnedKey = [u8; size_of::<Tick>() + size_of::<u64>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key = [0u8; size_of::<Tick>() + size_of::<u64>()];
        let (tick, token_id) = key.split_at_mut(size_of::<Tick>());
        tick.copy_from_slice(&self.tick);
        token_id.copy_from_slice(&self.token_id.to_be_bytes());
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (tick, token_id) = key_bytes.split_at(size_of::<Tick>());
        let tick = unsafe { Tick::new_unchecked(tick.try_into().unwrap()) };
        let token_id = u64::from_be_bytes(token_id.try_into().unwrap());
        Self { tick, token_id }
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct CurrentOwnershipValue {
    pub owner: ScriptPublicKey,
    pub mod_tx_score: u64,
}
pub type CurrentOwnershipPartition = Partition<OwnershipKey, CurrentOwnershipValue>;

#[derive(Copy, Clone, Debug)]
pub struct RangeKey {
    pub tick: Tick,
    pub index: u64,
}

impl Key for RangeKey {
    type OwnedKey = [u8; size_of::<Tick>() + size_of::<u64>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key = [0u8; size_of::<Tick>() + size_of::<u64>()];
        let (tick, index) = key.split_at_mut(size_of::<Tick>());
        tick.copy_from_slice(&self.tick);
        index.copy_from_slice(&self.index.to_be_bytes());
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (tick, index) = key_bytes.split_at(size_of::<Tick>());
        let tick = unsafe { Tick::new_unchecked(tick.try_into().unwrap()) };
        let index = u64::from_be_bytes(index.try_into().unwrap());
        Self { tick, index }
    }
}

impl AvailableRangesPartition {
    pub fn length(&self, tx: &mut WriteTransaction, tick: &Tick) -> Result<u64, Error> {
        let last_key = self
            .range_wtx(
                tx,
                RangeKey {
                    tick: *tick,
                    index: 0,
                }..=RangeKey {
                    tick: *tick,
                    index: u64::MAX,
                },
            )
            .next_back();
        match last_key {
            Some(Ok((k, _v))) => Ok(k.index + 1),
            Some(Err(e)) => Err(e),
            None => Ok(0),
        }
    }
}

pub type AvailableRangesPartition = Partition<RangeKey, (u64, u64)>;

pub type RangeLengthPartition = Partition<Tick, u64>;

#[derive(Copy, Clone, Debug)]
pub struct TokenMetaKey {
    pub tick: Tick,
    pub token_id: u64,
}

impl Key for TokenMetaKey {
    type OwnedKey = [u8; size_of::<Tick>() + size_of::<u64>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key = [0u8; size_of::<Tick>() + size_of::<u64>()];
        let (tick, token_id) = key.split_at_mut(size_of::<Tick>());
        tick.copy_from_slice(&self.tick);
        token_id.copy_from_slice(&self.token_id.to_be_bytes());
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (tick, token_id) = key_bytes.split_at(size_of::<Tick>());
        let tick = unsafe { Tick::new_unchecked(tick.try_into().unwrap()) };
        let token_id = u64::from_be_bytes(token_id.try_into().unwrap());
        Self { tick, token_id }
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
/// PreModRange tracks the original state of a range before modification when generating a token ID.
/// It stores the range's index, start, and size, along with the significant operation that was performed.
///
/// - Removed: The range was completely removed (only happens for size 1 ranges)
/// - Split: The range was split into two parts due to selecting a value in the middle
pub struct PreModRange {
    pub range_index: u64,
    pub start: u64,
    pub size: u64,
    pub removed: bool,
    pub split: bool,
    pub is_initial: bool,
}

pub type TokenIdMetaPartition = Partition<TokenMetaKey, PreModRange>;

#[derive(Clone, Debug)]
pub struct AddressHoldingKey {
    pub spk: ScriptPublicKey,
    pub tick: Tick,
    pub token_id: u64,
}

impl Key for AddressHoldingKey {
    type OwnedKey = SmallVec<[u8; 34 + size_of::<Tick>() + size_of::<u64>()]>;

    fn owned_key(&self) -> Self::OwnedKey {
        SmallVec::<[u8; 34 + size_of::<Tick>() + size_of::<u64>()]>::from_iter(
            self.spk
                .version
                .to_be_bytes()
                .into_iter()
                .chain(self.spk.script().iter().copied())
                .chain(self.tick.0.iter().copied())
                .chain(self.token_id.to_be_bytes().iter().copied()),
        )
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        // Cut token_id (u64) from the end
        let token_id_size = size_of::<u64>();
        let (remaining, token_id_bytes) = key_bytes.split_at(key_bytes.len() - token_id_size);
        let token_id = u64::from_be_bytes(token_id_bytes.try_into().unwrap());

        // Cut tick from the remaining bytes
        let tick_size = size_of::<Tick>();
        let (spk_bytes, tick_bytes) = remaining.split_at(remaining.len() - tick_size);
        let tick = unsafe { Tick::new_unchecked(tick_bytes.try_into().unwrap()) };

        // Remaining bytes are for ScriptPublicKey
        // First 2 bytes (u16) are version, rest is script
        let version = u16::from_be_bytes(spk_bytes[..2].try_into().unwrap());
        let script = SmallVec::from_slice(&spk_bytes[2..]);

        Self {
            spk: ScriptPublicKey::new(version, script),
            tick,
            token_id,
        }
    }
}

pub type ModTxScore = u64;
pub type AddressHoldingsPartition = Partition<AddressHoldingKey, ModTxScore>;

impl AddressHoldingsPartition {
    pub fn address_holdings_by_tick_rtx(
        &self,
        tx: &ReadTransaction,
        spk: &ScriptPublicKey,
        tick: &Tick,
    ) -> impl Iterator<Item = Result<u64, Error>> {
        let prefix: SmallVec<[u8; 34 + size_of::<Tick>()]> = spk
            .version
            .to_be_bytes()
            .into_iter()
            .chain(spk.script().iter().copied())
            .chain(tick.0.iter().copied())
            .collect();

        tx.prefix(&self.partition, prefix).map(|r| {
            r.map_err(Into::into).map(|(k, _v)| {
                let k = AddressHoldingKey::from_key_bytes(k.as_ref());
                k.token_id
            })
        })
    }
}

pub type NotificationQueuePartition = Partition<u64, VirtualChainChanges>;

impl NotificationQueuePartition {
    pub fn purge(&self, wtx: &mut fjall::WriteTransaction) -> Result<(), Error> {
        let queue = wtx.keys(&self.partition).collect::<Result<Vec<_>, _>>()?;
        queue.into_iter().try_for_each(|k| -> Result<(), Error> {
            wtx.remove(&self.partition, k);
            Ok(())
        })?;
        Ok(())
    }
}

pub type RejectionPartition = Partition<TransactionId, String>;

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum StatsKey {
    IndexerStats = 1,
}

impl From<u8> for StatsKey {
    fn from(key: u8) -> Self {
        // unsafe { std::mem::transmute(key) }
        match key {
            1 => StatsKey::IndexerStats,
            _ => panic!("Invalid StatsKey"),
        }
    }
}

pub type StatsPartition = Partition<StatsKey, Stats>;

impl Key for StatsKey {
    type OwnedKey = [u8; size_of::<StatsKey>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key = [0u8; size_of::<StatsKey>()];
        key[0] = *self as u8;
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        Self::from(key_bytes[0])
    }
}

impl StatsPartition {
    pub fn load(&self, tx: &ReadTransaction) -> Result<Stats, Error> {
        Ok(self
            .get_rtx(tx, &StatsKey::IndexerStats)?
            .unwrap_or_default())
    }

    pub fn addition(&self, tx: &mut WriteTransaction, diffs: StatsDiffs) -> Result<Stats, Error> {
        let stats = self
            .get_wtx(tx, &StatsKey::IndexerStats)?
            .unwrap_or_default();
        let stats = stats + diffs;
        self.insert_wtx(tx, StatsKey::IndexerStats, &stats)?;
        Ok(stats)
    }

    pub fn removal(&self, tx: &mut WriteTransaction, diffs: StatsDiffs) -> Result<Stats, Error> {
        let stats = self
            .get_wtx(tx, &StatsKey::IndexerStats)?
            .unwrap_or_default();
        let stats = stats - diffs;
        self.insert_wtx(tx, StatsKey::IndexerStats, &stats)?;
        Ok(stats)
    }
}

#[derive(Debug, Clone)]
pub struct ScoredDiscountKey {
    pub score: u64,
    pub tick: Tick,
    pub spk: ScriptPublicKey,
}

impl Key for ScoredDiscountKey {
    type OwnedKey = SmallVec<[u8; 34 + size_of::<Tick>() + size_of::<u64>()]>;

    fn owned_key(&self) -> Self::OwnedKey {
        Self::OwnedKey::from_iter(
            self.score
                .to_be_bytes()
                .into_iter()
                .chain(self.tick.0)
                .chain(self.spk.version.to_be_bytes())
                .chain(self.spk.script().iter().copied()),
        )
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (score, remaining) = key_bytes.split_at(size_of::<u64>());
        let (tick, spk) = remaining.split_at(size_of::<Tick>());
        // Remaining bytes are for ScriptPublicKey
        // First 2 bytes (u16) are version, rest is script
        let version = u16::from_be_bytes(spk[..2].try_into().unwrap());
        let script = SmallVec::from_slice(&spk[2..]);

        let score = u64::from_be_bytes(score.try_into().unwrap());
        let tick = unsafe { Tick::new_unchecked(tick.try_into().unwrap()) };
        Self {
            score,
            tick,
            spk: ScriptPublicKey::new(version, script),
        }
    }
}

pub type ScoredDiscountsPartition = Partition<ScoredDiscountKey, u64>;

impl ScoredDiscountsPartition {
    pub fn range_from_score_wtx<'a>(
        &'a self,
        wtx: &'a mut WriteTransaction,
        tx_score: u64,
    ) -> impl Iterator<Item = Result<(ScoredDiscountKey, u64), Error>> + 'a {
        wtx.range(&self.partition, tx_score.key()..).map(
            |rv| -> Result<(ScoredDiscountKey, u64), Error> {
                let (key_slice, value_slice) = rv?;
                let k = ScoredDiscountKey::from_key_bytes(key_slice.as_ref());
                let v = borsh::from_slice(value_slice.as_ref())
                    .map_err(|e| Error::Borsh(type_name::<Self>(), file!(), line!(), e))?;
                Ok((k, v))
            },
        )
    }
}

#[derive(Debug, Clone)]
pub struct VipKey {
    pub spk: ScriptPublicKey,
    pub tick: Tick,
    pub reversed_score: u64,
    pub fee: u64,
}

impl Key for VipKey {
    type OwnedKey = SmallVec<[u8; 34 + size_of::<Tick>() + size_of::<u64>() + size_of::<u64>()]>;

    fn owned_key(&self) -> Self::OwnedKey {
        Self::OwnedKey::from_iter(
            self.spk
                .version
                .to_be_bytes()
                .into_iter()
                .chain(self.spk.script().iter().copied())
                .chain(self.tick.0)
                .chain(self.reversed_score.to_be_bytes())
                .chain(self.fee.to_be_bytes()),
        )
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        // Cut fee (u64) from the end
        let fee_size = size_of::<u64>();
        let (remaining, fee_bytes) = key_bytes.split_at(key_bytes.len() - fee_size);
        let fee = u64::from_be_bytes(fee_bytes.try_into().unwrap());

        // Cut reversed_score (u64) from the remaining bytes
        let score_size = size_of::<u64>();
        let (remaining, score_bytes) = remaining.split_at(remaining.len() - score_size);
        let reversed_score = u64::from_be_bytes(score_bytes.try_into().unwrap());

        // Cut tick from the remaining bytes
        let tick_size = size_of::<Tick>();
        let (spk_bytes, tick_bytes) = remaining.split_at(remaining.len() - tick_size);
        let tick = unsafe { Tick::new_unchecked(tick_bytes.try_into().unwrap()) };

        // Remaining bytes are for ScriptPublicKey
        // First 2 bytes (u16) are version, rest is script
        let version = u16::from_be_bytes(spk_bytes[..2].try_into().unwrap());
        let script = SmallVec::from_slice(&spk_bytes[2..]);

        Self {
            spk: ScriptPublicKey::new(version, script),
            tick,
            reversed_score,
            fee,
        }
    }
}

pub type VipPartition = Partition<VipKey, ()>;

impl VipPartition {
    pub fn last_fee_wtx(
        &self,
        tx: &mut WriteTransaction,
        spk: &ScriptPublicKey,
        tick: &Tick,
    ) -> Result<Option<u64>, Error> {
        let key = self
            .range_keys_wtx(
                tx,
                VipKey {
                    spk: spk.clone(),
                    tick: *tick,
                    reversed_score: 0,
                    fee: 0,
                }..VipKey {
                    spk: spk.clone(),
                    tick: *tick,
                    reversed_score: u64::MAX,
                    fee: u64::MAX,
                },
            )
            .next()
            .transpose()?;
        Ok(key.map(|k| k.fee))
    }

    pub fn last_fee_rtx(
        &self,
        tx: &ReadTransaction,
        spk: &ScriptPublicKey,
        tick: &Tick,
    ) -> Result<Option<u64>, Error> {
        let key = self
            .range_keys_rtx(
                tx,
                VipKey {
                    spk: spk.clone(),
                    tick: *tick,
                    reversed_score: 0,
                    fee: 0,
                }..VipKey {
                    spk: spk.clone(),
                    tick: *tick,
                    reversed_score: u64::MAX,
                    fee: u64::MAX,
                },
            )
            .next()
            .transpose()?;

        Ok(key.map(|k| k.fee))
    }
}

pub type TxIDToRejectionPartition = Partition<TransactionId, String>;

impl TxIDToRejectionPartition {
    pub(crate) fn insert_rejection(
        &self,
        wtx: &mut WriteTransaction,
        key: TransactionId,
        reason: &String,
    ) -> Result<(), Error> {
        self.insert_wtx(wtx, key, reason)?;
        Ok(())
    }

    pub(crate) fn remove_rejection(
        &self,
        wtx: &mut WriteTransaction,
        key: TransactionId,
    ) -> Result<(), Error> {
        self.remove_wtx(wtx, &key)?;
        Ok(())
    }
}

pub type SerialToRejectedTxIDPartition = Partition<u64, TransactionId>;
impl SerialToRejectedTxIDPartition {
    pub(crate) fn insert_rejection(
        &self,
        wtx: &mut WriteTransaction,
        key: u64,
        txid: TransactionId,
    ) -> Result<(), Error> {
        self.insert_wtx(wtx, key, &txid)?;
        Ok(())
    }

    pub(crate) fn remove_rejection(
        &self,
        wtx: &mut WriteTransaction,
        key: u64,
    ) -> Result<(), Error> {
        self.remove_wtx(wtx, &key)
    }
}

// ================ MARKETPLACE LISTING PARTITIONS ================

/// Value stored for each active listing
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ListingValue {
    pub price: u64,
    pub seller: ScriptPublicKey,
    pub listing_tx_id: TransactionId,
    pub utxo_address: ScriptPublicKey,
    pub redeem_script: Vec<u8>,
    pub op_score: u64,
}

/// Primary listing lookup: Key = {tick}:{token_id}
/// Same key structure as OwnershipKey — one listing per token
pub type ListingsPartition = Partition<OwnershipKey, ListingValue>;

/// Sorted marketplace view: Key = {tick}:{price}:{token_id}
/// Allows querying listings for a collection sorted by price (ascending)
pub struct ListingByTickKey {
    pub tick: Tick,
    pub price: u64,
    pub token_id: u64,
}

impl Key for ListingByTickKey {
    type OwnedKey = [u8; size_of::<Tick>() + size_of::<u64>() + size_of::<u64>()];

    fn owned_key(&self) -> Self::OwnedKey {
        let mut key = [0u8; size_of::<Tick>() + size_of::<u64>() + size_of::<u64>()];
        let (tick, remaining) = key.split_at_mut(size_of::<Tick>());
        let (price, token_id) = remaining.split_at_mut(size_of::<u64>());
        tick.copy_from_slice(&self.tick);
        price.copy_from_slice(&self.price.to_be_bytes());
        token_id.copy_from_slice(&self.token_id.to_be_bytes());
        key
    }

    fn from_key_bytes(key_bytes: &[u8]) -> Self {
        let (tick, remaining) = key_bytes.split_at(size_of::<Tick>());
        let tick = unsafe { Tick::new_unchecked(tick.try_into().unwrap()) };
        let (price, token_id) = remaining.split_at(size_of::<u64>());
        let price = u64::from_be_bytes(price.try_into().unwrap());
        let token_id = u64::from_be_bytes(token_id.try_into().unwrap());
        Self {
            tick,
            price,
            token_id,
        }
    }
}

pub type ListingsByTickPartition = Partition<ListingByTickKey, ()>;

/// Seller's active listings: Key = {spk}:{tick}:{token_id}
/// Same key structure as AddressHoldingKey
pub type AddressListingsPartition = Partition<AddressHoldingKey, ModTxScore>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ownership_key_ordering() {
        let key1 = OwnershipKey {
            tick: "AAAA".parse().unwrap(),
            token_id: 1,
        };
        let key2 = OwnershipKey {
            tick: "AAAA".parse().unwrap(),
            token_id: 2,
        };
        let key3 = OwnershipKey {
            tick: "BBBB".parse().unwrap(),
            token_id: 1,
        };

        assert!(key1 < key2);
        assert!(key1 < key3);
        assert!(key2 < key3);
    }

    #[test]
    fn test_address_holding_key_roundtrip() {
        // Create test data
        let version = 1u16;
        let script = SmallVec::from_vec(vec![1, 2, 3, 4]);
        let spk = ScriptPublicKey::new(version, script);
        let tick = unsafe { Tick::new_unchecked([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]) };
        let token_id = 12345u64;

        let key = AddressHoldingKey {
            spk,
            tick,
            token_id,
        };

        // Serialize
        let serialized = key.owned_key();

        // Deserialize
        let deserialized = AddressHoldingKey::from_key_bytes(&serialized);

        // Verify all fields match
        assert_eq!(deserialized.spk.version(), key.spk.version());
        assert_eq!(deserialized.spk.script(), key.spk.script());
        assert_eq!(deserialized.tick.0, key.tick.0);
        assert_eq!(deserialized.token_id, key.token_id);
    }

    #[test]
    fn test_address_holding_key_byte_layout() {
        let version = 0xABCDu16;
        let script = SmallVec::from_vec(vec![0xD1, 0xD2, 0xD3, 0xD4]);
        let spk = ScriptPublicKey::new(version, script);
        let tick = unsafe {
            Tick::new_unchecked([0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA])
        };
        let token_id = 0x1234567890ABCDEFu64;

        let key = AddressHoldingKey {
            spk,
            tick,
            token_id,
        };

        let serialized = key.owned_key();

        // Check version bytes (first 2 bytes)
        assert_eq!(&serialized[0..2], &[0xAB, 0xCD]);

        // Check script bytes (next 4 bytes in this case)
        assert_eq!(&serialized[2..6], &[0xD1, 0xD2, 0xD3, 0xD4]);

        // Check tick bytes (next 8 bytes)
        assert_eq!(
            &serialized[6..16],
            &[0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA]
        );

        // Check token_id bytes (last 8 bytes)
        assert_eq!(
            &serialized[16..24],
            &[0x12, 0x34, 0x56, 0x78, 0x90, 0xAB, 0xCD, 0xEF]
        );
    }

    #[test]
    fn test_address_holding_key_with_empty_script() {
        let version = 1u16;
        let script = SmallVec::new();
        let spk = ScriptPublicKey::new(version, script);
        let tick = unsafe { Tick::new_unchecked([1; 10]) };
        let token_id = 0u64;

        let key = AddressHoldingKey {
            spk,
            tick,
            token_id,
        };

        let serialized = key.owned_key();
        let deserialized = AddressHoldingKey::from_key_bytes(&serialized);

        assert_eq!(deserialized.spk.version(), key.spk.version());
        assert_eq!(deserialized.spk.script(), key.spk.script());
        assert_eq!(deserialized.tick.0, key.tick.0);
        assert_eq!(deserialized.token_id, key.token_id);
    }

    #[test]
    fn test_address_holding_key_with_max_size_script() {
        let version = 1u16;
        let script = SmallVec::from_vec(vec![0xFF; 34]);
        let spk = ScriptPublicKey::new(version, script);
        let tick = unsafe { Tick::new_unchecked([2; 10]) };
        let token_id = u64::MAX;

        let key = AddressHoldingKey {
            spk,
            tick,
            token_id,
        };

        let serialized = key.owned_key();
        let deserialized = AddressHoldingKey::from_key_bytes(&serialized);

        assert_eq!(deserialized.spk.version(), key.spk.version());
        assert_eq!(deserialized.spk.script(), key.spk.script());
        assert_eq!(deserialized.tick.0, key.tick.0);
        assert_eq!(deserialized.token_id, key.token_id);
    }

    #[test]
    fn test_serialized_size() {
        let version = 1u16;
        let script = SmallVec::from_vec(vec![1, 2, 3, 4]);
        let spk = ScriptPublicKey::new(version, script.clone());
        let tick = unsafe { Tick::new_unchecked([1; 10]) };
        let token_id = 1u64;

        let key = AddressHoldingKey {
            spk,
            tick,
            token_id,
        };

        let serialized = key.owned_key();

        let expected_size = size_of::<u16>() + // version
                script.len() +        // script
                size_of::<Tick>() +   // tick
                size_of::<u64>(); // token_id

        assert_eq!(serialized.len(), expected_size);
    }

    #[test]
    fn test_ownership_history_key_serialization() {
        // Test different combinations of values
        let test_cases = vec![
            (Tick::MIN, 0, 0),
            (Tick::MAX, u64::MAX, u64::MAX),
            (
                unsafe { Tick::new_unchecked(*b"TEST\0\0\0\0\0\0") },
                12345,
                98765,
            ),
        ];

        for (tick, token_id, score) in test_cases {
            let key = OwnershipHistoryKey::with_score(tick, token_id, score);
            let owned_key = key.owned_key();
            let deserialized = OwnershipHistoryKey::from_key_bytes(&owned_key);

            assert_eq!(key.tick, deserialized.tick, "Tick mismatch");
            assert_eq!(key.token_id, deserialized.token_id, "Token ID mismatch");
            assert_eq!(
                key.reversed_score, deserialized.reversed_score,
                "Reversed score mismatch"
            );

            // Verify the score reversal logic
            assert_eq!(
                key.reversed_score,
                u64::MAX - score,
                "Score reversal incorrect"
            );
        }
    }

    // use krc721_core::network::Network;
    // use crate::database::Db;
    // #[test]
    // fn test_ownership_changes_ordering() -> Result<(), Error> {
    //     let db = Db::try_open(".test", &Network::Mainnet)?;
    //     let mut tx = db.write_tx();
    //
    //     // Create test data with increasing scores
    //     let tick = unsafe { Tick::new_unchecked(*b"TEST\0\0\0\0\0\0") };
    //     let test_data = vec![
    //         (1, 1000), // (token_id, score)
    //         (2, 1001),
    //         (3, 1002),
    //         (4, 1003),
    //         (5, 1004),
    //     ];
    //
    //     // Insert data
    //     for (token_id, score) in &test_data {
    //         let key = TokenMintsKey {
    //             score: *score,
    //             tick,
    //             token_id: *token_id,
    //             reversed_seq_number: 0,
    //         };
    //         db.ownership_changes.insert_wtx(&mut tx, key, &())?;
    //     }
    //
    //     // Query with threshold and verify ordering
    //     let threshold_score = 1001;
    //     let affected_keys = db
    //         .ownership_changes
    //         .range_keys_wtx(
    //             &mut tx,
    //             TokenMintsKey {
    //                 score: threshold_score,
    //                 tick: Tick::MIN,
    //                 token_id: 0,
    //                 reversed_seq_number: 0,
    //             }..,
    //         )
    //         .collect::<Result<Vec<_>, _>>()?;
    //
    //     // Verify we get entries with score >= threshold in ascending order
    //     let expected_token_ids: Vec<_> = test_data
    //         .iter()
    //         .filter(|(_, score)| *score >= threshold_score)
    //         .map(|(token_id, _)| *token_id)
    //         .collect();
    //
    //     let actual_token_ids: Vec<_> = affected_keys
    //         .iter()
    //         .map(|key| key.token_id)
    //         .collect();
    //
    //     assert_eq!(
    //         expected_token_ids, actual_token_ids,
    //         "Range query returned incorrect token IDs or wrong order"
    //     );
    //
    //     tx.rollback();
    //     Ok(())
    // }
}
