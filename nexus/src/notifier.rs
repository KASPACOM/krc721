use std::{
    collections::hash_map::Entry,
    hash::{Hash, Hasher},
};

#[allow(unused_imports)]
use crate::analyzer::detect_krc20;
use crate::analyzer::Analyzer;
use crate::imports::*;
use crate::syncer::process_acceptance_data;
use ahash::AHashSet;
use kaspa_rpc_core::VirtualChainChangedNotification;

struct Client {
    id: u64,
    interface: Arc<dyn ContextT>,
}

impl Client {
    fn new(interface: Arc<dyn ContextT>) -> Self {
        Self {
            id: interface.id(),
            interface,
        }
    }
}

impl Hash for Client {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for Client {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Client {}

struct Inner {
    pub address: RwLock<HashMap<Option<Address>, AHashSet<Client>>>,
    #[allow(unused)]
    pub krc20: RwLock<HashMap<Option<Address>, AHashSet<Client>>>,
    #[allow(unused)]
    pub krc721: RwLock<HashMap<Option<Address>, AHashSet<Client>>>,
    pub analyzer: Analyzer,
}

pub struct Notifier {
    #[allow(unused)]
    inner: Arc<Inner>,
}

impl Notifier {
    pub fn new(analyzer: Analyzer) -> Self {
        Self {
            inner: Arc::new(Inner {
                address: Default::default(),
                krc20: Default::default(),
                krc721: Default::default(),
                analyzer,
            }),
        }
    }

    pub fn subscribe(&self, subscription: Subscription, ctx: Arc<dyn ContextT>) -> Result<()> {
        match subscription {
            Subscription::Address { address_list } => {
                if let Some(address) = address_list {
                    for address in address {
                        match self
                            .inner
                            .address
                            .write()
                            .unwrap()
                            .entry(Some(address.clone()))
                        {
                            Entry::Occupied(mut entry) => {
                                entry.get_mut().insert(Client::new(ctx.clone()));
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(AHashSet::new());
                            }
                        }
                    }
                } else {
                    match self.inner.address.write().unwrap().entry(None) {
                        Entry::Occupied(mut entry) => {
                            entry.get_mut().insert(Client::new(ctx.clone()));
                        }
                        Entry::Vacant(entry) => {
                            entry.insert(AHashSet::new());
                        }
                    }
                }
            }
            _ => {
                return Err(Error::custom("not implemented"));
            } // Subscription::Krc20 { address_list } => {
              //     self.inner.krc20.entry(Some(address.clone())).or_insert_with(AHashSet::new).insert(ctx);
              // }
              // Subscription::Krc721 { address } => {
              //     self.inner.krc721.entry(Some(address.clone())).or_insert_with(AHashSet::new).insert(ctx);
              // }
        }
        Ok(())
    }

    pub async fn notify(&self, notification: Notification) -> Result<()> {
        match &notification {
            Notification::Test => {
                println!("test");
            }
            Notification::Address { address } => {
                let registry = self.inner.address.read().unwrap();
                registry.get(&Some(address.clone())).inspect(|client| {
                    client.iter().for_each(|client| {
                        client
                            .interface
                            .notify(&notification)
                            .map_err(|err| error!("{err}"))
                            .ok();
                    });
                });
                registry.get(&None).inspect(|client| {
                    client.iter().for_each(|client| {
                        client
                            .interface
                            .notify(&notification)
                            .map_err(|err| error!("{err}"))
                            .ok();
                    });
                });
                // println!("address");
            }
            Notification::Krc20Operation { .. } => {
                println!("krc20 operation");
            }
            Notification::Krc721Operation { .. } => {
                println!("krc721 operation");
            }
        }
        Ok(())
    }
}

impl ConsumerT for Notifier {
    fn handle_virtual_chain_changed(
        &self,
        VirtualChainChangedNotification {
            removed_chain_block_hashes: _,
            added_chain_block_hashes,
            added_acceptance_data,
        }: VirtualChainChangedNotification,
    ) -> Result<()> {
        let mergesets = process_acceptance_data(
            added_chain_block_hashes.as_slice(),
            added_acceptance_data.as_slice(),
            &self.inner.analyzer,
        );
        // TODO - STORE TO TEST DB

        println!("added chain block hashes: {:?}", added_chain_block_hashes);
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
