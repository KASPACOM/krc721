## Proctest Simple Add

Send to processor ```send_realtime_virtual_chain_changed_notification``` with ```VirtualChainChanges``` struct

Blue score starts from 1 and increases in each block

```rust
pub struct VirtualChainChanges {
    pub removed_chain_block_hashes: Arc<Vec<RpcHash>>, // Should be empty
    pub added_chain_block_hashes: Vec<(RpcHash, BlueScore)>, // Should not be empty
    pub scored_operations: Vec<(u64, NftOperation)>, // Should contain the NFT operation
}
```



