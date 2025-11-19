use crate::imports::*;
use crate::snapshot::*;
use fjall::{Config, PartitionCreateOptions};
use std::fs;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

fn temp_dir() -> PathBuf {
    tempfile::tempdir()
        .expect("Failed to obtain temporary directory")
        .path()
        .to_path_buf()
}

fn root() -> PathBuf {
    let database_root = temp_dir().join("test_snapshot");
    fs::create_dir_all(&database_root).expect("Failed to create database root");
    database_root
}

fn create_test_database(
    root: &Path,
    network_id: &Network,
    partitions: usize,
    records: usize,
    record_size: usize,
) -> Result<()> {
    let database_root = root.join(network_id.to_string());

    info!("Creating test database with {partitions} partitions and {records} records");

    let keyspace = Config::new(database_root).open()?;

    let partitions = (0..partitions as u64)
        .map(|i| keyspace.open_partition(&format!("{:016x}", i), PartitionCreateOptions::default()))
        .collect::<FjallResult<Vec<_>>>()?;

    let random_bytes: Vec<u8> = (0..record_size).map(|_| rand::random::<u8>()).collect();

    for partition in partitions {
        for i in 0..records {
            partition.insert(format!("{:016x}", i), &random_bytes)?;
        }
    }

    keyspace.persist(fjall::PersistMode::SyncAll)?;

    Ok(())
}

#[tokio::test]
async fn test_snapshot() -> Result<()> {
    const CHUNK_SIZE: usize = 1024 * 1024 * 4;
    const PARTITIONS: usize = 5;
    const RECORDS: usize = 1_000 * 10;
    const RECORD_SIZE: usize = 1024;

    let stdout_subscriber = tracing_subscriber::fmt::layer().without_time().with_filter(
        EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy(),
    );

    tracing_subscriber::registry()
        .with(stdout_subscriber)
        .init();

    let network = Network::from_str("testnet-10").unwrap();

    let root = root();
    info!(
        "Testing database snapshot with root folder: `{}`",
        root.display()
    );
    if fs::exists(&root)? {
        fs::remove_dir_all(&root).expect("Failed to remove temporary test root directory");
    }

    let source_dir = root.join("source");
    let destination_dir = root.join("destination");
    let archive_file = root.join("test_snapshot.krc721");

    create_test_database(&source_dir, &network, PARTITIONS, RECORDS, RECORD_SIZE)?;

    info!(
        "Creating snapshot archive file: `{}`",
        archive_file.display()
    );
    let header = Snapshot::default()
        .with_database(source_dir, &network)
        .with_archive(&archive_file)
        .with_chunk_size(CHUNK_SIZE)
        .archive_database()
        .await?;

    info!("Source archive header: {:?}", header);

    info!(
        "Restoring snapshot archive file: `{}`",
        archive_file.display()
    );
    let header = Snapshot::default()
        .with_database(destination_dir, &network)
        .with_archive(&archive_file)
        .restore()
        .await?;

    info!("Destination archive header: {:?}", header);

    Ok(())
}
