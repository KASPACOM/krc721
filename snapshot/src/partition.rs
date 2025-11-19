use crate::imports::*;
use fjall::{Keyspace, LsmError, Slice};

pub type LsmTreeIterator = Box<dyn Iterator<Item = Result<(Slice, Slice), LsmError>>>;
pub type PartitionId = u16;

struct PartitionInner {
    id: PartitionId,
    name: String,
    iter: Mutex<LsmTreeIterator>,
    #[allow(unused)]
    keyspace: Keyspace,
}

impl std::fmt::Debug for PartitionInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Partition {{ id: {}, name: {} }}", self.id, self.name)
    }
}

#[derive(Clone, Debug)]
pub struct Partition {
    inner: Rc<PartitionInner>,
}

impl Partition {
    pub fn try_open(
        id: PartitionId,
        name: &str,
        keyspace: Keyspace,
        snapshot: fjall::Snapshot,
    ) -> Result<Self> {
        // let handle = keyspace.open_partition(name, PartitionCreateOptions::default())?;
        let iter = Box::new(snapshot.iter()) as LsmTreeIterator;
        // Box::new(snapshot.iter()) as Box<dyn Iterator<Item = Result<(Slice, Slice), LsmError>>>;
        let iter = Mutex::new(iter);
        Ok(Self {
            inner: Rc::new(PartitionInner {
                id,
                name: name.to_string(),
                keyspace,
                // snapshot,
                iter,
            }),
        })
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn id(&self) -> u16 {
        self.inner.id
    }

    pub fn iter(&self) -> MutexGuard<'_, LsmTreeIterator> {
        self.inner.iter.lock().unwrap()
    }
}

#[derive(Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct PartitionTable {
    version: u16,
    partitions: Vec<(PartitionId, String)>,
}

impl TryFrom<&VecDeque<Partition>> for PartitionTable {
    type Error = Error;

    fn try_from(partitions: &VecDeque<Partition>) -> Result<Self> {
        if partitions.is_empty() {
            return Err(Error::custom("No partitions in database"));
        }

        Ok(Self {
            version: 1,
            partitions: partitions
                .iter()
                .map(|p| (p.id(), p.name().to_string()))
                .collect(),
        })
    }
}

impl PartitionTable {
    pub fn partitions(&self) -> &[(u16, String)] {
        &self.partitions
    }
}
