use crate::database::Db;
use crate::imports::*;
// use krc721_nexus::processor::VirtualChainChanges;
use kaspa_rpc_core::VirtualChainChangedNotification;
use krc721_nexus::analyzer::Analyzer;
use krc721_nexus::syncer::process_acceptance_data;

struct Inner {
    #[allow(unused)]
    db: Db,
    analyzer: Analyzer,
}

pub struct Recorder {
    #[allow(unused)]
    inner: Arc<Inner>,
}

impl Recorder {
    pub fn new(db: Db, analyzer: Analyzer) -> Self {
        Self {
            inner: Arc::new(Inner { db, analyzer }),
        }
    }
}

impl ConsumerT for Recorder {
    // temporarily (or permanently?) relocated to Processor from Nexus
    // to isolate Processor data ingest from Nexus logic allowing
    // Processor to receive notifications from different sources.
    fn handle_virtual_chain_changed(
        self: Arc<Self>,
        VirtualChainChangedNotification {
            removed_chain_block_hashes: _,
            added_chain_block_hashes: _,
            accepted_transaction_ids: _,
        }: VirtualChainChangedNotification,
    ) -> NexusResult<()> {
        let mergesets = process_acceptance_data(&[], &self.inner.analyzer);
        // TODO - STORE TO TEST DB

        println!("scored operations: {:?}", mergesets);

        Ok(())

        // if self.state.is_indexer_synced() {
        //     self.send_realtime_virtual_chain_changed_notification(VirtualChainChanges {
        //         removed_chain_block_hashes,
        //         added_chain_block_hashes,
        //         scored_operations,
        //     })
        //     .map_err(|_| crate::error::Error::SendError)?;
        //     Ok(())
        // } else {
        //     self.send_queued_virtual_chain_changed_notification(VirtualChainChanges {
        //         removed_chain_block_hashes,
        //         added_chain_block_hashes,
        //         scored_operations: vec![], // todo use scored_operations
        //     })
        //     .map_err(|_| crate::error::Error::SendError)?;
        //     Ok(())
        // }
    }
}
