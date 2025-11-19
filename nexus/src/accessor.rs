use std::time::SystemTime;

use crate::imports::*;
use crate::nft_view::{
    DbView, IteratorArgs as IteratorArgsView, SpkListLookupArgs, SpkLookupArgs, TokenHistoryRecord,
};
use crate::{calculate_tx_score_from_blue, nft_view};
use arc_swap::ArcSwapOption;
use kaspa_txscript::pay_to_address_script;
use krc721_core::model::krc721::model::*;
use krc721_core::model::krc721::*;
use krc721_core::network::Network;
use krc721_database::database::{CurrentOwnershipValue, Stats};
use krc721_database::prelude::Db;
use tap::TapOptional;
use tracing::{instrument, Instrument};

const MAX_ITERATOR_LIMIT: usize = 50;

struct Inner {
    #[allow(unused)] // TODO
    db: Arc<Db>,
    view: Arc<DbView>,
    state: Arc<State>,
    counters: Arc<Counters>,
    syncer: Option<Arc<dyn SyncerT>>,
    network: Network,
    address_prefix: Prefix,
    last_status_snapshot: ArcSwapOption<IndexerStatus>,
    last_status_snapshot_timestamp: AtomicU64,
    restricted_protocols: Arc<[String]>,
    reserved_tokens: Arc<[String]>,
}

#[derive(Clone)]
pub struct Accessor {
    inner: Arc<Inner>,
}

impl Accessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<Db>,
        view: Arc<DbView>,
        state: Arc<State>,
        counters: Arc<Counters>,
        syncer: Option<Arc<dyn SyncerT>>,
        network: Network,
        restricted_protocols: Arc<[String]>,
        reserved_tokens: Arc<[String]>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                view,
                state,
                counters,
                syncer,
                address_prefix: network.into(),
                network,
                last_status_snapshot: ArcSwapOption::new(None),
                last_status_snapshot_timestamp: AtomicU64::new(0),
                restricted_protocols,
                reserved_tokens,
            }),
        }
    }

    pub fn view(&self) -> &DbView {
        &self.inner.view
    }

    pub fn state(&self) -> &Arc<State> {
        &self.inner.state
    }

    pub fn counters(&self) -> &Arc<Counters> {
        &self.inner.counters
    }

    pub fn network(&self) -> &Network {
        &self.inner.network
    }

    pub fn syncer(&self) -> Option<Arc<dyn SyncerT>> {
        self.inner.syncer.clone()
    }

    pub fn address_prefix(&self) -> Prefix {
        self.inner.address_prefix
    }

    pub fn track_request(&self) {
        self.counters().requests.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl DataT for Accessor {
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_indexer_status(&self) -> CoreResult<Arc<IndexerStatus>> {
        self.track_request();

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let last_timestamp = self
            .inner
            .last_status_snapshot_timestamp
            .load(Ordering::SeqCst);
        let elapsed = now.saturating_sub(last_timestamp);

        // Return cached status if less than 1 second old
        if elapsed < 1000 {
            if let Some(status) = self.inner.last_status_snapshot.load().as_ref() {
                return Ok(status.clone());
            }
        }

        let tx = self.inner.db.read_tx();
        let Stats {
            deployments,
            mints,
            transfers,
            royalty_fees,
            security_fees,
        } = self.inner.db.stats.load(&tx).map_err(CoreError::custom)?;

        // Generate new status
        let status = {
            let last_known_block = self
                .syncer()
                .as_ref()
                .and_then(|syncer| syncer.last_known_block());
            let last_known_blue_score = last_known_block.map(|v| v.blue_score).unwrap_or_default();
            let current_op_score = calculate_tx_score_from_blue(last_known_blue_score + 1) - 1;
            let status = Arc::new(IndexerStatus {
                version: crate::VERSION.to_string(),
                network: self.inner.network,
                is_node_connected: self.state().is_node_connected(),
                is_node_synced: self.state().is_node_synced(),
                is_indexer_synced: self
                    .syncer()
                    .as_ref()
                    .tap_none(|| tracing::error!("syncer doesn't exist"))
                    .map(|syncer| {
                        let is_syncer_synced = syncer.is_synced();
                        if !is_syncer_synced {
                            warn!("syncer is not synced")
                        } else {
                            debug!("syncer is synced")
                        }
                        is_syncer_synced
                    })
                    .unwrap_or(false),
                last_known_block_hash: last_known_block.map(|v| v.block_hash),
                blue_score: last_known_blue_score,
                current_op_score,
                daa_score: self.state().current_daa_score(),
                // current_op_score: 0,
                pow_fees_total: security_fees,
                royalty_fees_total: royalty_fees,
                token_deployments_total: deployments,
                token_mints_total: mints,
                token_transfers_total: transfers,
            });

            self.inner.last_status_snapshot.store(Some(status.clone()));

            self.inner
                .last_status_snapshot_timestamp
                .store(now, Ordering::SeqCst);

            status
        };

        Ok(status)
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_collection_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<CollectionMetaWrapper>, Score>> {
        self.track_request();
        let address_prefix = self.address_prefix();
        let (offset, direction, limit) = resolve_u64_iter_args(iter_args);

        let iter_args = IteratorArgsView {
            offset,
            direction,
            limit,
        };

        // ---

        let view = self.view().clone();
        let restricted_protocols = self.inner.restricted_protocols.clone();
        let split_last = spawn_blocking(move || {
            view.krc721_collection_list(iter_args, restricted_protocols.as_ref())
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?;

        if let Some((collection, last)) = split_last {
            let collection = collection
                .into_iter()
                .map(|collection| CollectionMetaWrapper::try_from(collection, address_prefix))
                .collect::<Result<Vec<CollectionMetaWrapper>, _>>()?;
            let last = CollectionMetaWrapper::try_from(last, address_prefix)?;
            Ok(to_paginated(Some((collection, last)), limit, |last| {
                last.collection.op_score_added
            }))
        } else {
            Ok(Pagination {
                data: vec![],
                next_page_offset: None,
            })
        }
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_collection_lookup(
        &self,
        args: CollectionLookupArgs,
    ) -> CoreResult<Option<CollectionMetaWrapper>> {
        self.track_request();
        let address_prefix = self.address_prefix();

        let view = self.view().clone();
        let restricted_protocols = self.inner.restricted_protocols.clone();
        Ok(spawn_blocking(move || {
            view.krc721_collection_lookup(args, restricted_protocols.as_ref())
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?
        .map(|collection| CollectionMetaWrapper::try_from(collection, address_prefix))
        .transpose()
        .map_err(CoreError::custom)?)
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_token_list(
        &self,
        args: TokenListLookupArgs,
        iter_args: IteratorArgs<TokenId>,
    ) -> CoreResult<Pagination<Vec<Token>, TokenId>> {
        self.track_request();
        let restricted_protocols = self.inner.restricted_protocols.clone();
        let (offset, direction, limit) = resolve_u64_iter_args(iter_args);
        let TokenListLookupArgs { tick } = args;

        let view = self.view().clone();
        let skip_last = spawn_blocking(move || {
            view.krc721_token_list(
                tick,
                IteratorArgsView {
                    offset,
                    direction,
                    limit,
                },
                restricted_protocols.as_ref(),
            )
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?;

        let paginated = to_paginated(skip_last, limit, |ownership| ownership.token_id);
        let data = paginated
            .data
            .into_iter()
            .map(
                |nft_view::Ownership {
                     token_id,
                     owner,
                     mod_tx_score,
                 }|
                 -> CoreResult<_> {
                    let owner = extract_script_pub_key_address(&owner, self.address_prefix())
                        .map_err(CoreError::custom)?;
                    Ok(Token {
                        tick,
                        token_id,
                        owner,
                        op_score_modified: mod_tx_score,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let paginated = Pagination {
            data,
            next_page_offset: paginated.next_page_offset,
        };
        Ok(paginated)
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_token_lookup(&self, args: TokenLookupArgs) -> CoreResult<Option<Token>> {
        self.track_request();

        let view = self.view().clone();
        let restricted_protocols = self.inner.restricted_protocols.clone();
        let r =
            spawn_blocking(move || view.krc721_token_lookup(args, restricted_protocols.as_ref()))
                .in_current_span()
                .await
                .map_err(CoreError::custom)?
                .map_err(CoreError::custom)?;
        let r = r
            .map(
                |CurrentOwnershipValue {
                     owner,
                     mod_tx_score,
                 }| {
                    extract_script_pub_key_address(&owner, self.address_prefix())
                        .map(|address| (address, mod_tx_score))
                },
            )
            .transpose()
            .map_err(CoreError::custom)?
            .map(|(address, op_score_modified)| Token {
                tick: args.tick,
                token_id: args.id,
                owner: address,
                op_score_modified,
            });

        Ok(r)
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_address_nft_list(
        &self,
        lookup: AddressListLookupArgs,
        iter_args: IteratorArgs<TickTokenOffset>,
    ) -> CoreResult<Pagination<Vec<AddressNftInfo>, TickTokenOffset>> {
        self.track_request();
        let address = lookup.address; // it must be converted to address during parsing but who cares
        let address = address.try_into().map_err(CoreError::custom)?;
        let view = self.view().clone();
        let direction = iter_args.direction.unwrap_or_default();
        let restricted_protocols = self.inner.restricted_protocols.clone();

        let split_last = spawn_blocking(move || {
            view.krc721_address_nft_list(
                SpkListLookupArgs {
                    spk: pay_to_address_script(&address),
                },
                IteratorArgsView {
                    offset: iter_args.offset.unwrap_or(match direction {
                        Direction::Forward => TickTokenOffset {
                            tick: Tick::MIN,
                            token_id: TokenId::MIN,
                        },
                        Direction::Backward => TickTokenOffset {
                            tick: Tick::MAX,
                            token_id: TokenId::MAX,
                        },
                    }),
                    direction,
                    limit: iter_args
                        .limit
                        .unwrap_or(MAX_ITERATOR_LIMIT)
                        .min(MAX_ITERATOR_LIMIT),
                },
                restricted_protocols.as_ref(),
            )
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?;
        let paginated = to_paginated(
            split_last,
            iter_args
                .limit
                .unwrap_or(MAX_ITERATOR_LIMIT)
                .min(MAX_ITERATOR_LIMIT),
            |last| TickTokenOffset {
                tick: last.tick,
                token_id: last.token_id,
            },
        );
        Ok(paginated)
    }
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_address_nft_lookup(
        &self,
        args: AddressLookupArgs,
        iter_args: IteratorArgs<TokenId>,
    ) -> CoreResult<Pagination<Vec<AddressNftInfo>, TokenId>> {
        self.track_request();
        let view = self.view().clone();
        let address = args.address; // it must be converted to address during parsing but who cares
        let address = address.try_into().map_err(CoreError::custom)?;
        let restricted_protocols = self.inner.restricted_protocols.clone();
        let (offset, direction, limit) = resolve_u64_iter_args(iter_args);

        let split_last = spawn_blocking(move || {
            view.krc721_address_nft_lookup(
                SpkLookupArgs {
                    tick: args.tick,
                    spk: pay_to_address_script(&address),
                },
                IteratorArgsView {
                    offset,
                    direction,
                    limit,
                },
                restricted_protocols.as_ref(),
            )
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?;

        let paginated = to_paginated(split_last, limit, |last| last.token_id);
        let paginated = Pagination {
            data: paginated
                .data
                .into_iter()
                .map(|v| AddressNftInfo {
                    tick: args.tick,
                    tick_metadata: None,
                    token_id: v.token_id,
                    op_score_modified: v.mod_tx_score,
                })
                .collect(),
            next_page_offset: paginated.next_page_offset,
        };
        Ok(paginated)
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_op_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<OperationMetaWrapper>, Score>> {
        self.track_request();
        let address_prefix = self.address_prefix();
        let (offset, direction, limit) = resolve_u64_iter_args(iter_args);

        let view = self.view().clone();
        let skip_last = spawn_blocking(move || {
            view.krc721_op_list(IteratorArgsView {
                offset,
                direction,
                limit,
            })
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?;

        let paginated = to_paginated(skip_last, limit, |(score, _operation)| score);
        let data = paginated
            .data
            .into_iter()
            .map(|(score, op)| OperationMetaWrapper::try_from(score, op, address_prefix))
            .collect::<Result<Vec<OperationMetaWrapper>, _>>()
            .map_err(CoreError::custom)?;
        let paginated = Pagination {
            data,
            next_page_offset: paginated.next_page_offset,
        };
        Ok(paginated)
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_op_by_score(
        &self,
        args: OpLookupArgs,
    ) -> CoreResult<Option<OperationMetaWrapper>> {
        self.track_request();
        let address_prefix = self.address_prefix();
        let OpLookupArgs { score } = args;
        let view = self.view().clone();
        let r = spawn_blocking(move || view.krc721_op_by_score(score))
            .in_current_span()
            .await
            .map_err(CoreError::custom)?
            .map_err(CoreError::custom)?;
        let r = r
            .map(|op| OperationMetaWrapper::try_from(score, op, address_prefix))
            .transpose()
            .map_err(CoreError::custom)?;
        Ok(r)
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_op_by_txid(
        &self,
        args: TxLookupArgs,
    ) -> CoreResult<Option<OperationMetaWrapper>> {
        self.track_request();
        let address_prefix = self.address_prefix();
        let TxLookupArgs { txid } = args;
        let view = self.view().clone();
        let r = spawn_blocking(move || view.krc721_op_by_txid(txid))
            .in_current_span()
            .await
            .map_err(CoreError::custom)?
            .map_err(CoreError::custom)?;
        let r = r
            .map(|op| {
                OperationMetaWrapper::try_from(op.opscore, op.checked_operation, address_prefix)
            })
            .transpose()
            .map_err(CoreError::custom)?;
        Ok(r)
    }

    // PAGINATED
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_deployment_list(
        &self,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<ScoredDeployInfoWithCommon>, Score>> {
        self.track_request();
        let address_prefix = self.address_prefix();
        let restricted_protocols = self.inner.restricted_protocols.clone();
        let (offset, direction, limit) = resolve_u64_iter_args(iter_args);

        let view = self.view().clone();
        let skip_last = spawn_blocking(move || {
            view.krc721_deployment_list(
                IteratorArgsView {
                    offset,
                    direction,
                    limit,
                },
                restricted_protocols.as_ref(),
            )
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?;

        let paginated = to_paginated(skip_last, limit, |(score, _operation)| score);
        let data = paginated
            .data
            .into_iter()
            .map(|(score, op)| ScoredDeployInfoWithCommon::try_from(score, op, address_prefix))
            .collect::<Result<Vec<ScoredDeployInfoWithCommon>, _>>()
            .map_err(CoreError::custom)?;
        let paginated = Pagination {
            data,
            next_page_offset: paginated.next_page_offset,
        };
        Ok(paginated)
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_royalty_fee(&self, args: RoyaltyFeeLookupArgs) -> CoreResult<Option<String>> {
        self.track_request();
        let RoyaltyFeeLookupArgs { address, tick } = args;
        let address = address.try_into().map_err(CoreError::custom)?;

        let spk = pay_to_address_script(&address);

        let view = self.view().clone();
        let restricted_protocols = self.inner.restricted_protocols.clone();

        let r = spawn_blocking(move || {
            view.krc721_royalty_fee(spk, tick, restricted_protocols.as_ref())
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?;

        Ok(r.map(|fee| fee.to_string()))
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_rejection_by_txid(&self, args: TxLookupArgs) -> CoreResult<Option<String>> {
        self.track_request();

        let TxLookupArgs { txid } = args;

        let view = self.view().clone();
        let r = spawn_blocking(move || view.krc721_rejection_by_txid(txid))
            .in_current_span()
            .await
            .map_err(CoreError::custom)?
            .map_err(CoreError::custom)?;
        if let Some(rejection) = r {
            Ok(Some(rejection))
        } else {
            let view = self.view().clone();
            let r = spawn_blocking(move || view.krc721_op_by_txid(txid))
                .in_current_span()
                .await
                .map_err(CoreError::custom)?
                .map_err(CoreError::custom)?;

            if let Some(ScoredCheckedOperation {
                checked_operation:
                    CheckedOperation {
                        error: Some(error), ..
                    },
                ..
            }) = r
            {
                Ok(Some(error.to_string()))
            } else {
                Ok(None)
            }
        }
    }

    // STRUCT
    #[instrument(level = "error", skip(self), err)]
    async fn krc721_reserved_tokens(&self) -> CoreResult<Vec<String>> {
        self.track_request();
        Ok(self.inner.reserved_tokens.to_vec())
    }

    async fn krc721_token_history(
        &self,
        args: TokenLookupArgs,
        iter_args: IteratorArgs<Score>,
    ) -> CoreResult<Pagination<Vec<HistoryEntity>, Score>> {
        self.track_request();
        let address_prefix = self.address_prefix();
        let restricted_protocols = self.inner.restricted_protocols.clone();
        let (offset, direction, limit) = resolve_u64_iter_args(iter_args);

        let view = self.view().clone();
        let skip_last = spawn_blocking(move || {
            view.krc721_token_history_list(
                args,
                IteratorArgsView {
                    offset,
                    direction,
                    limit,
                },
                restricted_protocols.as_ref(),
            )
        })
        .in_current_span()
        .await
        .map_err(CoreError::custom)?
        .map_err(CoreError::custom)?;

        let paginated = to_paginated(skip_last, limit, |r| r.mod_tx_score);
        let data = paginated
            .data
            .into_iter()
            .map(
                |TokenHistoryRecord {
                     owner,
                     mod_tx_score,
                     tx_id,
                 }| {
                    let owner = extract_script_pub_key_address(&owner, address_prefix)?.to_string();
                    Ok(HistoryEntity {
                        owner,
                        op_score_modified: mod_tx_score,
                        tx_id,
                    })
                },
            )
            .collect::<Result<Vec<HistoryEntity>, _>>()
            .map_err(CoreError::custom::<TxScriptError>)?;
        let paginated = Pagination {
            data,
            next_page_offset: paginated.next_page_offset,
        };
        Ok(paginated)
    }

    #[instrument(level = "error", skip(self), err)]
    async fn krc721_available_token_id_ranges(
        &self,
        args: TokenListLookupArgs,
    ) -> CoreResult<Option<AvailableRanges>> {
        self.track_request();
        let view = self.view().clone();
        let ranges = spawn_blocking(move || view.krc721_available_token_id_ranges(args.tick))
            .in_current_span()
            .await
            .map_err(CoreError::custom)?
            .map_err(CoreError::custom)?;
        Ok(ranges)
    }
}

fn to_paginated<V, O>(
    split_last: Option<(Vec<V>, V)>,
    limit: usize,
    convert_element_to_offset: impl FnOnce(V) -> O,
) -> Pagination<Vec<V>, O> {
    match split_last {
        None => Pagination {
            data: vec![],
            next_page_offset: None,
        }, // data empty, offset empty, nothing is found
        Some((mut container, last_element)) if container.len() < limit => {
            container.push(last_element);
            Pagination {
                data: container,
                next_page_offset: None,
            }
        }
        Some((container, last_element)) => Pagination {
            data: container,
            next_page_offset: Some(convert_element_to_offset(last_element)),
        },
    }
}

fn resolve_u64_iter_args(iter_args: IteratorArgs<u64>) -> (Score, Direction, usize) {
    let direction = iter_args.direction.unwrap_or(Direction::Forward);
    let offset = iter_args.offset.unwrap_or(match direction {
        Direction::Forward => u64::MIN,
        Direction::Backward => u64::MAX,
    });
    let limit = iter_args
        .limit
        .unwrap_or(MAX_ITERATOR_LIMIT)
        .min(MAX_ITERATOR_LIMIT);
    (offset, direction, limit)
}

// fn resolve_iter_args(iter_args: IteratorArgs<Score>) -> (Score, Direction, usize) {
//     let direction = iter_args.direction.unwrap_or(Direction::Forward);
//     let offset = iter_args.offset.unwrap_or(match direction {
//         Direction::Forward => Score::MIN,
//         Direction::Backward => Score::MAX,
//     });
//     let limit = iter_args
//         .limit
//         .unwrap_or(MAX_ITERATOR_LIMIT)
//         .min(MAX_ITERATOR_LIMIT);
//     (offset, direction, limit)
// }
