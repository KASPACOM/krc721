use crate::error::Error;
use crate::result::Result;
use crate::utils::prelude::SplitLastItertools;
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionId};
use krc721_core::model::krc721::{
    AddressNftInfo, AvailableRange, AvailableRanges, CheckedOperation, Collection,
    CollectionLookupArgs, CollectionState, DeployInfoWithCommon, Direction, Score,
    ScoredCheckedOperation, Tick, TickTokenOffset, TokenId, TokenLookupArgs,
};
use krc721_database::database::{
    AddressHoldingKey, CurrentOwnershipValue, Db, DeploymentKey, ListingByTickKey, ListingValue,
    OwnershipHistoryKey, OwnershipKey, RangeKey, ScoredDeployInfoWithCommon,
};
use krc721_database::prelude::ReadTransaction;
use std::sync::Arc;
use tracing::{instrument, trace};

// todo remove filtering by restricted protocols !!!

pub const MAX_LIMIT: usize = 50;

pub struct IteratorArgs<Offset = Score> {
    pub offset: Offset,
    pub direction: Direction,
    pub limit: usize,
}

pub struct SpkListLookupArgs {
    pub spk: ScriptPublicKey,
}

pub struct SpkLookupArgs {
    pub tick: Tick,
    pub spk: ScriptPublicKey,
}

pub struct NftLookupEntry {
    pub token_id: u64,
    pub mod_tx_score: u64,
}

pub struct Ownership {
    pub token_id: u64,
    pub owner: ScriptPublicKey,
    pub mod_tx_score: u64,
}

pub struct TokenHistoryRecord {
    pub owner: ScriptPublicKey,
    pub mod_tx_score: u64,
    pub tx_id: TransactionId,
}

#[derive(Clone)]
pub struct DbView {
    db: Arc<Db>,
}

impl DbView {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    fn process_iter<I, T, F, K, V>(
        &self,
        rtx: &ReadTransaction,
        iter: I,
        limit: usize,
        mut f: F,
    ) -> Result<Option<(Vec<T>, T)>, Error>
    where
        I: Iterator<Item = Result<(K, V), krc721_database::error::Error>>,
        F: for<'a> FnMut(&'a ReadTransaction, K, V) -> Result<T>,
    {
        iter.map(|result| {
            result
                .map_err(Error::from)
                .and_then(|(key, info)| f(rtx, key, info))
        })
        .take(limit + 1)
        .collect_split_last_result()
    }

    // ---------------------
    // --- API ENDPOINTS ---
    // ---------------------

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_collection_list(
        &self,
        IteratorArgs {
            offset,
            direction,
            limit,
        }: IteratorArgs<Score>,
        restricted_protocols: &[String],
    ) -> Result<Option<(Vec<Collection>, Collection)>> {
        let rtx = &self.db.read_tx();
        let process_fn = |rtx: &ReadTransaction,
                          DeploymentKey { score, tick }: DeploymentKey,
                          info: DeployInfoWithCommon| {
            let last_mint_seq = self
                .db
                .mint_history
                .last_minted_token_seq_no_rtx(rtx, &tick)?;
            let minted = last_mint_seq.map(|t| t.seq_no).unwrap_or_default();
            let mts_mod = last_mint_seq
                .map(|t| {
                    self.db
                        .operation_history
                        .get_rtx(rtx, &t.score)
                        .transpose()
                        .unwrap()
                        .map(|v| v.operation.common.block_time)
                })
                .transpose()?;

            Ok(Collection {
                minted,
                op_score_modified: last_mint_seq.map(|t| t.score).unwrap_or(score),
                state: CollectionState::Deployed,
                mts_mod: mts_mod.unwrap_or(info.common.block_time),
                op_score_added: score,
                deploy_info_with_common: info,
            })
        };

        // let list = match direction.unwrap_or(Direction::Forward) {
        let list = match direction {
            Direction::Forward => self.process_iter(
                rtx,
                self.db
                    .collection_deployments
                    .range_rtx(
                        rtx,
                        DeploymentKey {
                            score: offset,
                            tick: Tick::MIN,
                        }..,
                    )
                    .filter(|v| {
                        !v.as_ref()
                            .is_ok_and(|(_, v)| v.has_incompatible_uri_prefix(restricted_protocols))
                    }),
                limit,
                process_fn,
            ),
            Direction::Backward => self.process_iter(
                rtx,
                self.db
                    .collection_deployments
                    .range_rtx(
                        rtx,
                        ..=DeploymentKey {
                            score: offset,
                            tick: Tick::MAX,
                        },
                    )
                    .rev()
                    .filter(|v| {
                        !v.as_ref()
                            .is_ok_and(|(_, v)| v.has_incompatible_uri_prefix(restricted_protocols))
                    }),
                limit,
                process_fn,
            ),
        }?;
        Ok(list)
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_collection_lookup(
        &self,
        CollectionLookupArgs { tick }: CollectionLookupArgs,
        restricted_protocols: &[String],
    ) -> Result<Option<Collection>> {
        let rtx = self.db.read_tx();
        let collection = self.db.collection_registry.get_rtx(&rtx, &tick)?;
        let Some(collection) = collection else {
            return Ok(None);
        };
        if collection
            .info
            .has_incompatible_uri_prefix(restricted_protocols)
        {
            return Ok(None);
        }
        let ScoredDeployInfoWithCommon {
            score: deploy_score,
            info,
        } = collection;
        let last_mint_seq = self
            .db
            .mint_history
            .last_minted_token_seq_no_rtx(&rtx, &tick)?;
        let minted = last_mint_seq.map(|t| t.seq_no).unwrap_or_default();
        let mts_mod = last_mint_seq
            .map(|t| -> Result<Option<u64>> {
                Ok(self
                    .db
                    .operation_history
                    .get_rtx(&rtx, &t.score)?
                    .map(|v| v.operation.common.block_time))
            })
            .transpose()?
            .flatten();

        Ok(Some(Collection {
            minted,
            op_score_modified: last_mint_seq.map(|t| t.score).unwrap_or(deploy_score),
            state: CollectionState::Deployed,
            mts_mod: mts_mod.unwrap_or(info.common.block_time),
            op_score_added: deploy_score,
            deploy_info_with_common: info,
        }))
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_token_lookup(
        &self,
        TokenLookupArgs { tick, id }: TokenLookupArgs,
        restricted_protocols: &[String],
    ) -> Result<Option<CurrentOwnershipValue>> {
        let rtx = self.db.read_tx();
        let v = self
            .db
            .current_ownership
            .get_rtx(&rtx, &OwnershipKey { tick, token_id: id })?;
        let Some(v) = v else { return Ok(None) };
        let dinfo = self.db.collection_registry.get_rtx(&rtx, &tick)?;
        let Some(dinfo) = dinfo else {
            // todo error
            return Ok(None);
        };
        if dinfo.info.has_incompatible_uri_prefix(restricted_protocols) {
            return Ok(None); // todo log
        }
        Ok(Some(v))
    }

    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_token_list(
        &self,
        tick: Tick,
        IteratorArgs {
            offset,
            direction,
            limit,
        }: IteratorArgs<TokenId>,
        restricted_protocols: &[String],
    ) -> Result<Option<(Vec<Ownership>, Ownership)>> {
        let rtx = &self.db.read_tx();
        let dinfo = self.db.collection_registry.get_rtx(rtx, &tick)?;
        let Some(dinfo) = dinfo else {
            // todo error
            return Ok(None);
        };
        if dinfo.info.has_incompatible_uri_prefix(restricted_protocols) {
            return Ok(None); // todo log
        }
        let processed_fn = |_rtx: &ReadTransaction,
                            OwnershipKey { token_id, .. }: OwnershipKey,
                            CurrentOwnershipValue {
                                owner,
                                mod_tx_score,
                            }: CurrentOwnershipValue| {
            Ok(Ownership {
                token_id,
                owner,
                mod_tx_score,
            })
        };
        match direction {
            Direction::Forward => Ok(self.process_iter(
                rtx,
                self.db.current_ownership.range_rtx(
                    rtx,
                    OwnershipKey {
                        tick,
                        token_id: offset,
                    }..=OwnershipKey {
                        tick,
                        token_id: u64::MAX,
                    },
                ),
                limit,
                processed_fn,
            )?),
            Direction::Backward => Ok(self.process_iter(
                rtx,
                self.db
                    .current_ownership
                    .range_rtx(
                        rtx,
                        OwnershipKey { tick, token_id: 0 }..=OwnershipKey {
                            tick,
                            token_id: offset,
                        },
                    )
                    .rev(),
                limit,
                processed_fn,
            )?),
        }
    }

    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_token_history_list(
        &self,
        TokenLookupArgs { tick, id }: TokenLookupArgs,
        IteratorArgs {
            offset,
            direction,
            limit,
        }: IteratorArgs<Score>,
        restricted_protocols: &[String],
    ) -> Result<Option<(Vec<TokenHistoryRecord>, TokenHistoryRecord)>> {
        let rtx = &self.db.read_tx();
        let dinfo = self.db.collection_registry.get_rtx(rtx, &tick)?;
        let Some(dinfo) = dinfo else {
            // todo error
            return Ok(None);
        };
        if dinfo.info.has_incompatible_uri_prefix(restricted_protocols) {
            return Ok(None); // todo log
        }

        let processed_fn =
            |rtx: &ReadTransaction, k: OwnershipHistoryKey, owner: ScriptPublicKey| {
                let score = k.score();
                let tx_id = self
                    .db
                    .operation_history
                    .get_rtx(rtx, &score)?
                    .expect("operation must exist")
                    .operation
                    .common
                    .tx_id;

                Ok(TokenHistoryRecord {
                    owner,
                    mod_tx_score: score,
                    tx_id,
                })
            };
        match direction {
            Direction::Forward => Ok(self.process_iter(
                rtx,
                self.db
                    .ownership_history
                    .range_rtx(
                        rtx,
                        OwnershipHistoryKey::new(tick, id, 0)
                            ..=OwnershipHistoryKey::with_score(tick, id, offset),
                    )
                    .rev(),
                limit,
                processed_fn,
            )?),
            Direction::Backward => Ok(self.process_iter(
                rtx,
                self.db.ownership_history.range_rtx(
                    rtx,
                    OwnershipHistoryKey::with_score(tick, id, offset)
                        ..=OwnershipHistoryKey::new(tick, id, u64::MAX),
                ),
                limit,
                processed_fn,
            )?),
        }
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_address_nft_list(
        &self,
        SpkListLookupArgs { spk }: SpkListLookupArgs,
        IteratorArgs {
            offset: TickTokenOffset { tick, token_id },
            direction,
            limit,
        }: IteratorArgs<TickTokenOffset>,
        restricted_protocols: &[String],
    ) -> Result<Option<(Vec<AddressNftInfo>, AddressNftInfo)>> {
        let rtx = &self.db.read_tx();
        let mut c = Vec::with_capacity(limit + 1);
        let last_tick_info = &mut ScoredDeployInfoWithCommon::default();
        let mut last_incompatible = false;
        let mut conditional_push =
            |r: Result<(AddressHoldingKey, u64), krc721_database::error::Error>| -> Result<bool> {
                let (
                    AddressHoldingKey {
                        spk: _,
                        tick,
                        token_id,
                    },
                    mod_tx_score,
                ) = r?;
                if last_tick_info.info.common.tick == tick && !last_incompatible {
                    c.push(AddressNftInfo {
                        tick,
                        tick_metadata: Some(last_tick_info.info.info.metadata.clone()),
                        token_id,
                        op_score_modified: mod_tx_score,
                    })
                } else if last_tick_info.info.common.tick != tick {
                    let depl = self.db.collection_registry.get_rtx(rtx, &tick)?.unwrap();
                    last_incompatible = depl.info.has_incompatible_uri_prefix(restricted_protocols);
                    *last_tick_info = depl;
                    if !last_incompatible {
                        c.push(AddressNftInfo {
                            tick,
                            tick_metadata: Some(last_tick_info.info.info.metadata.clone()),
                            token_id,
                            op_score_modified: mod_tx_score,
                        })
                    }
                }
                if c.len() < limit + 1 {
                    Ok(true)
                } else {
                    Ok(false)
                }
            };
        match direction {
            Direction::Forward => {
                for r in self.db.address_holdings.range_rtx(
                    rtx,
                    AddressHoldingKey {
                        spk: spk.clone(),
                        tick,
                        token_id,
                    }..AddressHoldingKey {
                        spk: spk.clone(),
                        tick: Tick::MAX,
                        token_id: u64::MAX,
                    },
                ) {
                    if !conditional_push(r)? {
                        break;
                    }
                }
            }
            Direction::Backward => {
                for r in self
                    .db
                    .address_holdings
                    .range_rtx(
                        rtx,
                        AddressHoldingKey {
                            spk: spk.clone(),
                            tick: Tick::MIN,
                            token_id: 0,
                        }..=AddressHoldingKey {
                            spk,
                            tick,
                            token_id,
                        },
                    )
                    .rev()
                {
                    if !conditional_push(r)? {
                        break;
                    }
                }
            }
        };
        let last = c.pop();
        Ok(last.map(|v| (c, v)))
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_address_nft_lookup(
        &self,
        SpkLookupArgs { tick, spk }: SpkLookupArgs,
        IteratorArgs {
            offset: token_id,
            direction,
            limit,
        }: IteratorArgs<TokenId>,
        restricted_protocols: &[String],
    ) -> Result<Option<(Vec<NftLookupEntry>, NftLookupEntry)>> {
        let rtx = &self.db.read_tx();
        let dinfo = self.db.collection_registry.get_rtx(rtx, &tick)?;
        let Some(dinfo) = dinfo else {
            // todo error
            return Ok(None);
        };
        if dinfo.info.has_incompatible_uri_prefix(restricted_protocols) {
            return Ok(None); // todo log
        }

        let processed_fn = |_rtx: &ReadTransaction,
                            AddressHoldingKey { token_id, spk, .. }: AddressHoldingKey,
                            mod_tx_score: u64| {
            trace!("spk is {spk:?}");
            Ok(NftLookupEntry {
                token_id,
                mod_tx_score,
            })
        };
        match direction {
            Direction::Forward => Ok(self.process_iter(
                rtx,
                self.db.address_holdings.range_rtx(
                    rtx,
                    AddressHoldingKey {
                        spk: spk.clone(),
                        tick,
                        token_id,
                    }..AddressHoldingKey {
                        spk,
                        tick,
                        token_id: u64::MAX,
                    },
                ),
                limit,
                processed_fn,
            )?),
            Direction::Backward => Ok(self.process_iter(
                rtx,
                self.db
                    .address_holdings
                    .range_rtx(
                        rtx,
                        AddressHoldingKey {
                            spk: spk.clone(),
                            tick,
                            token_id: u64::MIN,
                        }..=AddressHoldingKey {
                            spk,
                            tick,
                            token_id,
                        },
                    )
                    .rev(),
                limit,
                processed_fn,
            )?),
        }
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    #[allow(clippy::type_complexity)]
    pub fn krc721_op_list(
        &self,
        IteratorArgs {
            offset,
            direction,
            limit,
        }: IteratorArgs<Score>,
    ) -> Result<Option<(Vec<(Score, CheckedOperation)>, (Score, CheckedOperation))>> {
        let rtx = &self.db.read_tx();
        let process_fn =
            |_rtx: &ReadTransaction, score: Score, op: CheckedOperation| Ok((score, op));

        match direction {
            Direction::Forward => Ok(self.process_iter(
                rtx,
                self.db.operation_history.range_rtx(rtx, offset..),
                limit,
                process_fn,
            )?),
            Direction::Backward => Ok(self.process_iter(
                rtx,
                self.db.operation_history.range_rtx(rtx, ..=offset).rev(),
                limit,
                process_fn,
            )?),
        }
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_op_by_score(&self, score: u64) -> Result<Option<CheckedOperation>> {
        let rtx = self.db.read_tx();

        let data = self.db.operation_history.get_rtx(&rtx, &score)?;

        Ok(data)
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_op_by_txid(&self, txid: TransactionId) -> Result<Option<ScoredCheckedOperation>> {
        let rtx = self.db.read_tx();
        // todo check if static errors partition has the tx
        let Some(score) = self.db.tx_id_to_opscore.get_rtx(&rtx, &txid)? else {
            return Ok(None);
        };
        let data = self.db.operation_history.get_rtx(&rtx, &score)?;
        Ok(data.map(|op| ScoredCheckedOperation {
            opscore: score,
            checked_operation: op,
        }))
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    #[allow(clippy::type_complexity)]
    pub fn krc721_deployment_list(
        &self,
        IteratorArgs {
            offset,
            direction,
            limit,
        }: IteratorArgs<Score>,
        restricted_protocols: &[String],
    ) -> Result<
        Option<(
            Vec<(Score, DeployInfoWithCommon)>,
            (Score, DeployInfoWithCommon),
        )>,
    > {
        let rtx = &self.db.read_tx();
        let process_fn = |_rtx: &ReadTransaction, key: DeploymentKey, op: DeployInfoWithCommon| {
            Ok((key.score, op))
        };

        match direction {
            Direction::Forward => Ok(self.process_iter(
                rtx,
                self.db
                    .collection_deployments
                    .range_rtx(
                        rtx,
                        DeploymentKey {
                            score: offset,
                            tick: Tick::MIN,
                        }..,
                    )
                    .filter(|v| {
                        !v.as_ref()
                            .is_ok_and(|(_, v)| v.has_incompatible_uri_prefix(restricted_protocols))
                    }),
                limit,
                process_fn,
            )?),
            Direction::Backward => Ok(self.process_iter(
                rtx,
                self.db
                    .collection_deployments
                    .range_rtx(
                        rtx,
                        ..=DeploymentKey {
                            score: offset,
                            tick: Tick::MAX,
                        },
                    )
                    .rev()
                    .filter(|v| {
                        !v.as_ref()
                            .is_ok_and(|(_, v)| v.has_incompatible_uri_prefix(restricted_protocols))
                    }),
                limit,
                process_fn,
            )?),
        }
    }

    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_royalty_fee(
        &self,
        spk: ScriptPublicKey,
        tick: Tick,
        restricted_protocols: &[String],
    ) -> Result<Option<u64>> {
        let rtx = self.db.read_tx();
        let collection = self.db.collection_registry.get_rtx(&rtx, &tick)?;
        let Some(collection) = collection else {
            return Ok(None);
        };
        if collection
            .info
            .has_incompatible_uri_prefix(restricted_protocols)
        {
            return Ok(None); // todo log
        }

        Ok(self
            .db
            .vip
            .last_fee_rtx(&rtx, &spk, &tick)?
            .or_else(|| collection.info.info.royalty.map(|royalty| royalty.fee)))
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_rejection_by_txid(&self, txid: TransactionId) -> Result<Option<String>> {
        let rtx = self.db.read_tx();
        Ok(self.db.tx_id_to_rejection.get_rtx(&rtx, &txid)?)
    }

    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_available_token_id_ranges(&self, tick: Tick) -> Result<Option<AvailableRanges>> {
        let rtx = &self.db.read_tx();
        let depl = self.db.collection_registry.get_rtx(rtx, &tick)?;
        let Some(depl) = depl else {
            return Ok(None);
        };
        let last_mint_seq = self
            .db
            .mint_history
            .last_minted_token_seq_no_rtx(rtx, &tick)?;
        let Some(last_mint_seq) = last_mint_seq else {
            let max = depl.info.info.max;
            if max == 0 {
                return Ok(Some(AvailableRanges::FullyMinted));
            }
            return Ok(Some(AvailableRanges::Available(vec![AvailableRange {
                start_token_id: 1,
                size: depl.info.info.max,
            }])));
        };
        if last_mint_seq.seq_no == depl.info.info.max {
            return Ok(Some(AvailableRanges::FullyMinted));
        }
        let ranges = self
            .db
            .available_ranges
            .range_rtx(
                rtx,
                RangeKey { tick, index: 0 }..RangeKey {
                    tick,
                    index: u64::MAX,
                },
            )
            .map(|r| {
                r.map(|(_, (start, size))| AvailableRange {
                    start_token_id: start,
                    size,
                })
                .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;
        if ranges.is_empty() && depl.info.info.premint < depl.info.info.max {
            Ok(Some(AvailableRanges::Available(vec![AvailableRange {
                start_token_id: depl.info.info.premint + 1,
                size: depl.info.info.max - depl.info.info.premint,
            }])))
        } else {
            Ok(Some(AvailableRanges::Available(ranges)))
        }
    }

    // ---------------------
    // --- MARKETPLACE ---
    // ---------------------

    /// Get active listings for a collection, sorted by price (ascending)
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_active_listings(
        &self,
        tick: Tick,
        IteratorArgs {
            offset,
            direction,
            limit,
        }: IteratorArgs<Score>,
    ) -> Result<Option<(Vec<ListingEntry>, ListingEntry)>> {
        let rtx = &self.db.read_tx();
        // Verify collection exists
        if self.db.collection_registry.get_rtx(rtx, &tick)?.is_none() {
            return Ok(None);
        }

        let processed_fn = |_rtx: &ReadTransaction,
                            key: ListingByTickKey,
                            _: ()| {
            // Look up the full listing data
            let listing = self
                .db
                .listings
                .get_rtx(_rtx, &OwnershipKey { tick: key.tick, token_id: key.token_id })?
                .ok_or(Error::custom("listing index inconsistency"))?;
            Ok(ListingEntry {
                tick: key.tick,
                token_id: key.token_id,
                price: key.price,
                seller: listing.seller,
                listing_tx_id: listing.listing_tx_id,
                redeem_script: listing.redeem_script,
                op_score: listing.op_score,
            })
        };

        // Use price-sorted index. Offset is op_score but we use price=0 as start
        match direction {
            Direction::Forward => Ok(self.process_iter(
                rtx,
                self.db.listings_by_tick.range_rtx(
                    rtx,
                    ListingByTickKey { tick, price: offset, token_id: 0 }
                        ..=ListingByTickKey { tick, price: u64::MAX, token_id: u64::MAX },
                ),
                limit,
                processed_fn,
            )?),
            Direction::Backward => Ok(self.process_iter(
                rtx,
                self.db.listings_by_tick.range_rtx(
                    rtx,
                    ListingByTickKey { tick, price: 0, token_id: 0 }
                        ..=ListingByTickKey { tick, price: offset, token_id: u64::MAX },
                ).rev(),
                limit,
                processed_fn,
            )?),
        }
    }

    /// Look up a single listing by tick and token_id
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_listing_lookup(
        &self,
        tick: Tick,
        token_id: u64,
    ) -> Result<Option<ListingValue>> {
        let rtx = self.db.read_tx();
        Ok(self.db.listings.get_rtx(&rtx, &OwnershipKey { tick, token_id })?)
    }

    /// Get active listings for an address
    #[instrument(level = "error", skip(self), err)]
    pub fn krc721_address_listings(
        &self,
        spk: &ScriptPublicKey,
        IteratorArgs {
            offset,
            direction,
            limit,
        }: IteratorArgs<TickTokenOffset>,
    ) -> Result<Option<(Vec<ListingEntry>, ListingEntry)>> {
        let rtx = &self.db.read_tx();

        let processed_fn = |_rtx: &ReadTransaction,
                            key: AddressHoldingKey,
                            _score: u64| {
            let listing = self
                .db
                .listings
                .get_rtx(_rtx, &OwnershipKey { tick: key.tick, token_id: key.token_id })?
                .ok_or(Error::custom("listing index inconsistency"))?;
            Ok(ListingEntry {
                tick: key.tick,
                token_id: key.token_id,
                price: listing.price,
                seller: listing.seller,
                listing_tx_id: listing.listing_tx_id,
                redeem_script: listing.redeem_script,
                op_score: listing.op_score,
            })
        };

        match direction {
            Direction::Forward => Ok(self.process_iter(
                rtx,
                self.db.address_listings.range_rtx(
                    rtx,
                    AddressHoldingKey { spk: spk.clone(), tick: offset.tick, token_id: offset.token_id }
                        ..=AddressHoldingKey { spk: spk.clone(), tick: Tick::MAX, token_id: u64::MAX },
                ),
                limit,
                processed_fn,
            )?),
            Direction::Backward => Ok(self.process_iter(
                rtx,
                self.db.address_listings.range_rtx(
                    rtx,
                    AddressHoldingKey { spk: spk.clone(), tick: Tick::MIN, token_id: 0 }
                        ..=AddressHoldingKey { spk: spk.clone(), tick: offset.tick, token_id: offset.token_id },
                ).rev(),
                limit,
                processed_fn,
            )?),
        }
    }
}

/// A listing entry returned from view queries
pub struct ListingEntry {
    pub tick: Tick,
    pub token_id: u64,
    pub price: u64,
    pub seller: ScriptPublicKey,
    pub listing_tx_id: TransactionId,
    pub redeem_script: Vec<u8>,
    pub op_score: u64,
}
