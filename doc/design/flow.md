# KRC-721 Indexer Serialization Documentation

## Overview

The KRC-721 indexer implements a sophisticated serialization mechanism to handle both historical and real-time blockchain data while maintaining consistency. The system is designed to process blockchain events in the correct order, handle reorgs, and ensure data integrity during the initial sync and ongoing operations.

## Architecture Components

### Key Components

1. **Processor**: Handles the core processing logic for chain changes
2. **Syncer**: Manages synchronization with the blockchain node
3. **Queue System**: Handles real-time notifications during initial sync
4. **State Management**: Tracks sync status and processes notifications accordingly

## Serialization Process

### Initial State Processing

1. **Last Known Block**
    - The syncer starts from a known block hash (either initial or last accepted)
    - This block serves as the synchronization starting point

2. **Historical Data Processing**
   ```rust
   // Syncer requests historical data from this point
   let from = *self.last_known_block.lock().unwrap();
   let historical_data = self.bridge.get_historical_data(from).await;
   ```

3. **Sync Status Tracking**
    - The system maintains a sync status flag (`is_synced`)
    - This flag determines how incoming notifications are processed
   ```rust
   self.is_synced.store(true, Ordering::SeqCst);
   ```

### Real-time Queue Management

During initial sync, real-time notifications are handled specially:

1. **Queue Creation**
    - Real-time notifications are queued while historical sync is in progress
   ```rust
   fn process_queue(&self, vcc: VirtualChainChanges) -> Result<()> {
       let mut wtx = self.db.write_tx();
       let next_key = self.db.notification_queue
           .last_key_wtx(&mut wtx)?
           .unwrap_or_default() + 1;
       self.db.notification_queue.insert_wtx(&mut wtx, next_key, &vcc)?;
   ```

2. **Queue Application**
    - Once historical sync reaches the target block, queued notifications are processed
   ```rust
   fn process_queue_application(&self) -> Result<()> {
       for i in first..=last {
           let vcc = self.db.notification_queue
               .remove_if_exists_wtx(&mut wtx, &i)?
               .unwrap();
           self.process_chain_changes_wtx(vcc, &mut wtx)?;
       }
   ```

### Post-Sync Operation

After initial sync is complete:

1. **Real-time Processing**
    - New notifications are processed immediately
   ```rust
   if self.is_synced.load(Ordering::SeqCst) {
       self.processor.send_realtime_virtual_chain_changed_notification(notification)
   ```

2. **Chain Reorganization Handling**
    - System handles chain reorganizations by:
        - Removing invalidated blocks
        - Recalculating affected state
        - Reprocessing new chain tip

## Chain Changes Processing

The system processes chain changes in two phases:

### Phase 1: Reorg Handling
```rust
fn process_removal(&self, tx: &mut WriteTransaction, removed_blocks: &[RpcHash]) -> Result<()> {
    // 1. Identify affected blocks
    // 2. Remove affected blocks from chain state
    // 3. Calculate transaction score threshold
    // 4. Remove affected NFT operations
    // 5. Remove affected deployments
    // 6. Reconstruct token ownership
}
```

### Phase 2: New Block Processing
```rust
fn process_additions(&self, tx: &mut WriteTransaction, mergesets: Vec<Mergeset>) -> Result<()> {
    // 1. Add new blocks to chain state
    // 2. Process NFT operations in order
    // 3. Update state based on operations
}
```

## Consistency Guarantees

The system maintains several consistency guarantees:

1. **Ordering Guarantee**
    - Historical data is processed before queued real-time notifications
    - Operations within blocks are processed in order

2. **State Consistency**
    - Database transactions ensure atomic updates
    - Reorg handling maintains state consistency

3. **Error Handling**
    - Failed operations are recorded and can be audited
    - System can recover from interruptions

## Best Practices for Developers

1. **State Monitoring**
    - Monitor the `is_synced` flag for system status
    - Use appropriate notification handling based on sync status

2. **Error Handling**
    - Implement proper error handling for all operations
    - Log errors appropriately for debugging

3. **Transaction Management**
    - Use write transactions appropriately
    - Ensure proper commit/rollback handling

4. **Testing**
    - Test reorg scenarios thoroughly
    - Verify queue processing behavior
    - Validate state consistency after operations

## Common Pitfalls

1. **Race Conditions**
    - Always use proper synchronization primitives
    - Be careful with shared state access

2. **Memory Management**
    - Monitor queue size during long sync periods
    - Implement proper cleanup mechanisms

3. **Error Propagation**
    - Ensure errors are properly propagated
    - Don't swallow critical errors

## Conclusion

The KRC-721 indexer serialization system provides a robust mechanism for handling both historical and real-time blockchain data. Understanding these mechanisms is crucial for maintaining and extending the system effectively.