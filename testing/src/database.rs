use crate::imports::*;

#[allow(unused_imports)]
use fjall::{Config, PartitionCreateOptions, ReadTransaction, TxKeyspace};

struct Inner {
    #[allow(unused)]
    keyspace: TxKeyspace,
}

pub struct Db {
    #[allow(unused)]
    inner: Arc<Inner>,
}

impl Db {
    pub fn new(name: &str) -> Self {
        let keyspace = Config::new(name).open_transactional().unwrap();

        Self {
            inner: Arc::new(Inner { keyspace }),
        }
    }
}
