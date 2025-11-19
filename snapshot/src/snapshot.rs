use crate::imports::*;
use crate::partition::PartitionId;
use crate::progress::Progress;
use fjall::{Config, PartitionCreateOptions};
use futures::stream::{FuturesOrdered, Stream, StreamExt};
use krc721_core::network::Network;
use krc721_database::database as db;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Seek, SeekFrom};
use std::{pin::Pin, task::Poll};
use tokio::task;
use tracing::info;

pub(crate) const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024 * 4;
pub(crate) const HEADER_MAGIC: &[u8; 8] = b"KSPR-721";
pub(crate) const DATA_OFFSET: u64 = 128;

pub(crate) type FjallResult<T> = std::result::Result<T, fjall::Error>;

pub struct Context {
    partitions: VecDeque<Partition>,
    archive_filename: PathBuf,
}

#[derive(Default)]
pub struct Snapshot {
    database_root: Option<PathBuf>,
    archive_filename: Option<PathBuf>,
    chunk_size: Option<usize>,
    progress: Option<Arc<Progress>>,
    skip_partitions: Option<Vec<&'static str>>,
}

impl Snapshot {
    pub fn with_database<P: AsRef<Path>>(self, data_dir: P, network: &Network) -> Self {
        let database_root = data_dir.as_ref().join(network.to_string());
        Self {
            database_root: Some(database_root),
            ..self
        }
    }

    pub fn with_archive<P: AsRef<Path>>(self, archive_filename: P) -> Self {
        let archive_filename = archive_filename.as_ref();
        Self {
            archive_filename: Some(archive_filename.to_path_buf()),
            ..self
        }
    }

    pub fn with_chunk_size(self, chunk_size: usize) -> Self {
        Self {
            chunk_size: Some(chunk_size),
            ..self
        }
    }

    pub fn with_progress(self, progress: Arc<Progress>) -> Self {
        Self {
            progress: Some(progress),
            ..self
        }
    }

    pub fn skip_partitions(self, skip_partitions: Vec<&'static str>) -> Self {
        Self {
            skip_partitions: Some(skip_partitions),
            ..self
        }
    }

    pub fn purge(&self) -> Result<()> {
        let database_root = self
            .database_root
            .as_ref()
            .ok_or(Error::custom("Please specify database filename"))?;
        fs::remove_dir_all(database_root)?;
        Ok(())
    }

    pub async fn archive_database(&self) -> Result<Header> {
        let context = self.context_from_database()?;
        self.archive_from_context(context).await
    }

    pub async fn archive_snapshots(
        &self,
        partition_snapshots: impl IntoIterator<Item = db::Snapshot>,
    ) -> Result<Header> {
        let context = self.context_from_snapshots(partition_snapshots)?;
        self.archive_from_context(context).await
    }

    fn context_from_database(&self) -> Result<Context> {
        let database_root = self
            .database_root
            .as_ref()
            .ok_or(Error::custom("Please specify database filename"))?;
        let archive_filename = self
            .archive_filename
            .clone()
            .ok_or(Error::custom("Please specify archive filename"))?;

        if fs::exists(&archive_filename)? {
            return Err(Error::custom(
                "Archive file already exists, please remove it before packing.",
            ));
        }

        // Open database, generate partition table and a VecDeque of partitions
        let keyspace = Config::new(database_root).open()?;

        let partitions = keyspace
            .list_partitions()
            .iter()
            .enumerate()
            .map(|(i, p)| (i as u16, p.to_string()))
            .collect::<Vec<_>>();

        if partitions.is_empty() {
            return Err(Error::custom(
                "No partitions found in the database... ¯\\_(ツ)_/¯",
            ));
        }

        let partitions = partitions
            .iter()
            .map(|(id, name)| {
                keyspace
                    .open_partition(name, PartitionCreateOptions::default())
                    .map(|handle| (id, name, handle))
                    .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;

        let instant = keyspace.instant();
        let partitions = partitions
            .into_iter()
            .map(|(id, name, handle)| (id, name, handle.snapshot_at(instant)))
            .collect::<Vec<_>>();

        let partitions = partitions
            .into_iter()
            .map(|(id, name, snapshot)| Partition::try_open(*id, name, keyspace.clone(), snapshot))
            .collect::<Result<VecDeque<_>>>()?;

        let context = Context {
            partitions,
            archive_filename,
        };

        Ok(context)
    }

    pub(crate) fn context_from_snapshots(
        &self,
        snapshots: impl IntoIterator<Item = db::Snapshot>,
    ) -> Result<Context> {
        let archive_filename = self
            .archive_filename
            .clone()
            .ok_or(Error::custom("Please specify archive filename"))?;

        if fs::exists(&archive_filename)? {
            return Err(Error::custom(
                "Archive file already exists, please remove it before packing.",
            ));
        }

        let partitions = snapshots
            .into_iter()
            .filter(|snapshot| {
                !self
                    .skip_partitions
                    .as_ref()
                    .map(|list| list.contains(&snapshot.name.as_ref()))
                    .unwrap_or(false)
            })
            .enumerate()
            .map(
                |(
                    id,
                    db::Snapshot {
                        keyspace,
                        name,
                        snapshot,
                    },
                )| {
                    Partition::try_open(id as PartitionId, &name, keyspace, snapshot)
                },
            )
            .collect::<Result<VecDeque<_>>>()?;

        let context = Context {
            partitions,
            archive_filename,
        };

        Ok(context)
    }

    /// Pack the database into an archive file
    pub(crate) async fn archive_from_context(&self, context: Context) -> Result<Header> {
        let Context {
            partitions,
            archive_filename,
        } = context;

        let partition_table = PartitionTable::try_from(&partitions)?;

        // Create archive file and data offset padding for the Header
        let mut archive_file = fs::File::create(archive_filename)?;
        let zeros = [0u8; DATA_OFFSET as usize];
        archive_file.write_all(&zeros)?;
        partition_table.serialize(&mut archive_file)?;

        // Ingest database partitions and write compressed chunks to the archive file
        let mut ingest = DatabaseIngest::new(partitions, self.chunk_size, self.progress.clone());
        let worker_count = std::thread::available_parallelism()?.get();
        let mut pending_tasks = FuturesOrdered::new();
        let mut hasher = Sha256::new();
        let mut chunks = 0u64;

        loop {
            while pending_tasks.len() < worker_count {
                if let Some(chunk) = ingest.next().await {
                    let chunk = chunk?;
                    chunks += 1;

                    let task = task::spawn_blocking(move || chunk.deflate());

                    pending_tasks.push_back(task);
                } else {
                    break;
                }
            }

            // Exit if no more tasks
            if pending_tasks.is_empty() {
                break;
            }

            // Wait for next completed task and write it
            if let Some(result) = pending_tasks.next().await {
                let deflated = result??;
                deflated.write(&mut archive_file)?;
                hasher.update(deflated.data());
            }
        }

        // release snapshots as soon as we are
        // done with the archive generation
        drop(ingest);

        let hash = hasher.finalize();

        // Update header with the final chunk count
        archive_file.seek(SeekFrom::Start(0))?;
        let header = Header {
            chunks,
            hash: hash.into(),
            ..Header::default()
        };
        header.serialize(&mut archive_file)?;

        archive_file.flush()?;
        archive_file.sync_all()?;
        drop(archive_file);

        Ok(header)
    }

    /// Unpack the archive file into the database
    pub async fn restore(&self) -> Result<Header> {
        let database_root = self
            .database_root
            .as_ref()
            .ok_or(Error::custom("Please specify database filename"))?;

        if fs::exists(database_root)? {
            return Err(Error::custom(
                "Database already exists, please remove it before restoring an archive.",
            ));
        }

        let archive_filename = self
            .archive_filename
            .as_ref()
            .ok_or(Error::custom("Please specify archive filename"))?;

        if !fs::exists(archive_filename)? {
            return Err(Error::custom(
                "Unable to locate archive file at `{archive_filename}`",
            ));
        }

        let mut archive_file = fs::File::open(archive_filename)?;
        let header = Header::deserialize_reader(&mut archive_file)?;
        let Header {
            magic,
            chunks,
            hash,
            offset,
            ..
        } = header;
        if magic != *HEADER_MAGIC {
            return Err(Error::custom("Invalid archive file"));
        }
        archive_file.seek(SeekFrom::Start(offset))?;

        let partition_table = PartitionTable::deserialize_reader(&mut archive_file)?;
        let keyspace = Config::new(database_root).open()?;
        let partition_map = partition_table
            .partitions()
            .iter()
            .map(|(id, name)| {
                keyspace
                    .open_partition(name, PartitionCreateOptions::default())
                    .map(|handle| (*id, handle))
            })
            .collect::<FjallResult<HashMap<_, _>>>()?;

        let mut hasher = Sha256::new();

        for n in 0..chunks {
            let deflated = Deflated::read(&mut archive_file)?;
            hasher.update(deflated.data());

            let Inflated {
                partition_id,
                records,
            } = deflated.inflate()?;

            let partition = partition_map
                .get(&partition_id)
                .ok_or(Error::custom("Invalid partition id"))?;
            for record in records {
                partition.insert(record.k, record.v)?;
            }

            if let Some(progress) = self.progress.as_ref() {
                progress.update(n as f64 / chunks as f64);
            }
        }

        let local_hash = hasher.finalize();
        if local_hash != hash.into() {
            return Err(Error::custom("Invalid archive file"));
        }

        Ok(header)
    }
}

struct DatabaseIngest {
    partitions: RefCell<VecDeque<Partition>>,
    chunk_size: Option<usize>,
    progress: Option<Arc<Progress>>,
    total: usize,
}

impl DatabaseIngest {
    fn new(
        partitions: VecDeque<Partition>,
        chunk_size: Option<usize>,
        progress: Option<Arc<Progress>>,
    ) -> Self {
        if let Some(partition) = partitions.front() {
            if let Some(progress) = progress.as_ref() {
                progress.message(partition.name());
            } else {
                info!("processing `{}`", partition.name());
            }
        }

        let total = partitions.len();

        Self {
            partitions: RefCell::new(partitions),
            chunk_size,
            progress,
            total,
        }
    }
}

impl Stream for DatabaseIngest {
    type Item = Result<Inflated>;

    fn poll_next(
        self: Pin<&mut Self>,
        _ctx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut partitions = self.partitions.borrow_mut();

        while let Some(partition) = partitions.front().cloned() {
            if let Some(chunk) =
                Inflated::fill(&partition, self.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE))
                    .transpose()
            {
                return Poll::Ready(Some(chunk));
            } else {
                partitions.pop_front();
                if let Some(partition) = partitions.front() {
                    if let Some(progress) = self.progress.as_ref() {
                        progress.message(partition.name());
                    } else {
                        info!("processing `{}`", partition.name());
                    }
                }

                if let Some(progress) = self.progress.as_ref() {
                    progress.update((self.total - partitions.len()) as f64 / self.total as f64);
                }
            }
        }

        Poll::Ready(None)
    }
}
