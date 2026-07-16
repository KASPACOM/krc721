use kaspa_consensus_core::config::bps::Bps;

const VERSION: &str = env!("CARGO_PKG_VERSION");

cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {

        pub mod context;
        pub mod error;
        pub mod imports;
        #[allow(clippy::module_inception)]
        pub mod nexus;
        pub mod event;
        pub mod analyzer;
        pub mod result;
        pub mod processor;
        pub mod accessor;
        pub mod utils;
        pub mod state;
        pub mod metrics;
        pub mod nft_view;
        pub mod bridge;
        pub mod consumer;
        pub mod syncer;
        pub mod notifier;

        pub mod prelude {
            pub use crate::nexus::Nexus;
            pub use crate::bridge::{RpcBridge, BridgeT};
            pub use crate::metrics::Metrics;
            pub use crate::state::State;
            pub use crate::context::ContextT;
            pub use crate::processor::Processor;
            pub use crate::accessor::Accessor;
            pub use crate::syncer::{Syncer,SyncerT};
        }
    }
}

const MERGE_SET_LIMIT: u64 = Bps::<10>::mergeset_size_limit();
const BLOCK_TX_CAPACITY: u64 = 1000;

/// Calculate transaction score threshold for dependent data cleanup
pub fn calculate_tx_score_from_blue(blue_score: u64) -> u64 {
    calculate_tx_score(blue_score, 0, 0)
}

/// Convert a blue score to its first operation score, rejecting overflow.
pub fn checked_tx_score_from_blue(blue_score: u64) -> Option<u64> {
    blue_score
        .checked_mul(MERGE_SET_LIMIT)?
        .checked_mul(BLOCK_TX_CAPACITY)
}

pub fn calculate_tx_score(
    blue_score: u64,
    block_tx_index_within_mergeset: u64,
    tx_index_within_merged_block: u64,
) -> u64 {
    blue_score * MERGE_SET_LIMIT * BLOCK_TX_CAPACITY
        + (block_tx_index_within_mergeset * BLOCK_TX_CAPACITY)
        + tx_index_within_merged_block
}

pub fn calculate_blue_score_from_tx_score(tx_score: u64) -> u64 {
    tx_score / (MERGE_SET_LIMIT * BLOCK_TX_CAPACITY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_blue_score_conversion_rejects_overflow() {
        assert_eq!(
            checked_tx_score_from_blue(42),
            Some(calculate_tx_score_from_blue(42))
        );
        assert_eq!(checked_tx_score_from_blue(u64::MAX), None);
    }
}
