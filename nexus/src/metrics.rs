use crate::imports::*;
use krc721_core::network::Network;
use krc721_database::database::{Db, Stats};
use portable_atomic::{AtomicF64, AtomicUsize};
use statsd::Client;

use workflow_perf_monitor::{
    cpu::{processor_numbers, ProcessStat},
    fd::fd_count_cur,
    io::{get_process_io_stats, IOStats},
    mem::{get_process_memory_info, ProcessMemoryInfo},
};

// When adding a new counter it must be added to:
// - Counters as an atomic
// - Snapshot as f64
// - Rates as f64
// - Snapshot From<&Counters> impl
// - Rates From<&Snapshot, &Snapshot, f64> impl

#[derive(Default)]
pub struct Counters {
    pub test: AtomicU64,
    pub requests: AtomicU64,
    pub vcc_notifications: AtomicU64,
    pub db_disk_space_bytes: AtomicU64,

    /// this is the non-swapped physical memory a process has used.
    /// On UNIX it matches `top`'s RES column.
    ///
    /// On Windows this is an alias for wset field and it matches "Mem Usage"
    /// column of taskmgr.exe.
    pub resident_memory: AtomicU64,
    /// this is the total amount of virtual memory used by the process.
    /// On UNIX it matches `top`'s VIRT column.
    ///
    /// On Windows this is an alias for pagefile field and it matches "Mem
    /// Usage" "VM Size" column of taskmgr.exe.
    pub virtual_memory: AtomicU64,

    // pub core_num: AtomicUsize,
    pub core_num: usize,
    pub cpu_usage: AtomicF64,

    pub fd_num: AtomicUsize,

    pub disk_io_read_bytes: AtomicU64,
    pub disk_io_write_bytes: AtomicU64,
    // pub disk_io_read_per_sec: AtomicF64,
    // pub disk_io_write_per_sec: AtomicF64,
    pub total_deployments: AtomicU64,
    pub total_mints: AtomicU64,
    pub total_transfers: AtomicU64,
    pub total_royalty_fees: AtomicU64,
    pub total_security_fees: AtomicU64,
}

impl Counters {
    pub fn new(core_num: usize) -> Self {
        Self {
            core_num,
            ..Default::default()
        }
    }

    pub fn update_from_stats(&self, stats: &Stats) {
        self.total_deployments
            .store(stats.deployments, Ordering::Release);
        self.total_mints.store(stats.mints, Ordering::Release);
        self.total_transfers
            .store(stats.transfers, Ordering::Release);
        self.total_royalty_fees
            .store(stats.royalty_fees, Ordering::Release);
        self.total_security_fees
            .store(stats.security_fees, Ordering::Release);
    }
}

#[derive(Default)]
struct Snapshot {
    requests: f64,
    vcc_notifications: f64,
    db_disk_space_bytes: f64,
    resident_memory: f64,
    virtual_memory: f64,
    cpu_usage: f64,
    fd_num: f64,
    disk_io_read_bytes: f64,
    disk_io_write_bytes: f64,
    total_deployments: f64,
    total_mints: f64,
    total_transfers: f64,
    total_royalty_fees: f64,
    total_security_fees: f64,
}

impl From<&Counters> for Snapshot {
    fn from(counters: &Counters) -> Self {
        Self {
            requests: counters.requests.load(Ordering::SeqCst) as f64,
            vcc_notifications: counters.vcc_notifications.load(Ordering::SeqCst) as f64,
            db_disk_space_bytes: counters.db_disk_space_bytes.load(Ordering::SeqCst) as f64,
            resident_memory: counters.resident_memory.load(Ordering::SeqCst) as f64,
            virtual_memory: counters.virtual_memory.load(Ordering::SeqCst) as f64,
            cpu_usage: counters.cpu_usage.load(Ordering::SeqCst) / counters.core_num as f64,
            fd_num: counters.fd_num.load(Ordering::SeqCst) as f64,
            disk_io_read_bytes: counters.disk_io_read_bytes.load(Ordering::SeqCst) as f64,
            disk_io_write_bytes: counters.disk_io_write_bytes.load(Ordering::SeqCst) as f64,
            total_deployments: counters.total_deployments.load(Ordering::SeqCst) as f64,
            total_mints: counters.total_mints.load(Ordering::SeqCst) as f64,
            total_transfers: counters.total_transfers.load(Ordering::SeqCst) as f64,
            total_royalty_fees: counters.total_royalty_fees.load(Ordering::SeqCst) as f64,
            total_security_fees: counters.total_security_fees.load(Ordering::SeqCst) as f64,
        }
    }
}

impl Snapshot {
    fn iter(&self) -> impl Iterator<Item = (&'static str, f64)> {
        [
            ("counters.requests", self.requests),
            ("counters.vcc_notifications", self.vcc_notifications),
            ("system.cpu", self.cpu_usage),
            ("system.resident", self.resident_memory),
            ("system.virtual", self.virtual_memory),
            ("system.fd", self.fd_num),
            ("system.disk_io_read_bytes", self.disk_io_read_bytes),
            ("system.disk_io_write_bytes", self.disk_io_write_bytes),
            ("system.db_disk_space_bytes", self.db_disk_space_bytes),
            ("indexer.total_deployments", self.total_deployments),
            ("indexer.total_mints", self.total_mints),
            ("indexer.total_transfers", self.total_transfers),
            ("indexer.total_royalty_fees", self.total_royalty_fees),
            ("indexer.total_security_fees", self.total_security_fees),
        ]
        .into_iter()
    }
}

struct Rates {
    requests: f64,
    vcc_notifications: f64,
    disk_io_read_bytes_per_sec: f64,
    disk_io_write_bytes_per_sec: f64,
}

impl Rates {
    fn from(a: &Snapshot, b: &Snapshot, period_secs: f64) -> Self {
        Self {
            requests: (b.requests - a.requests) / period_secs,
            vcc_notifications: (b.vcc_notifications - a.vcc_notifications) / period_secs,
            disk_io_read_bytes_per_sec: (b.disk_io_read_bytes - a.disk_io_read_bytes) / period_secs,
            disk_io_write_bytes_per_sec: (b.disk_io_write_bytes - a.disk_io_write_bytes)
                / period_secs,
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&'static str, f64)> {
        [
            ("rates.requests", self.requests),
            ("rates.vcc_notifications", self.vcc_notifications),
            (
                "rates.disk_io_read_bytes_per_sec",
                self.disk_io_read_bytes_per_sec,
            ),
            (
                "rates.disk_io_write_bytes_per_sec",
                self.disk_io_write_bytes_per_sec,
            ),
        ]
        .into_iter()
    }
}

pub struct Metrics {
    pub db: Arc<Db>,
    pub counters: Arc<Counters>,
    pub shutdown: DuplexChannel<()>,
    pub client: Option<Client>,
}

impl Metrics {
    pub fn try_new(db: Arc<Db>, network: Network) -> Result<Self> {
        let hostname = hostname::get()
            .map_err(|e| Error::custom(format!("Failed to get hostname: {}", e)))?
            .to_string_lossy()
            .replace(".", "-")
            .to_string();

        let client = Client::new(
            "graphite.krc721.stream:8125",
            format!("{hostname}.krc721d-{network}").as_str(),
        )
        .map_err(|e| error!("Failed to create statsd client: {}", e))
        .ok();

        let counters = Counters::new(processor_numbers()?);

        Ok(Self {
            db,
            counters: Arc::new(counters),
            shutdown: DuplexChannel::oneshot(),
            client,
        })
    }

    pub fn statsd(&self) -> Option<&Client> {
        self.client.as_ref()
    }

    pub fn counters(&self) -> &Arc<Counters> {
        &self.counters
    }

    // Post metrics to statsd
    // https://crates.io/crates/statsd
    async fn post(&self, snapshot: &Snapshot, rates: Rates) {
        if let Some(client) = self.client.as_ref() {
            snapshot.iter().for_each(|(name, value)| {
                client.gauge(name, value);
            });

            rates.iter().for_each(|(name, value)| {
                client.gauge(name, value);
            });

            // client.gauge("counters.requests", snapshot.requests);
            // client.gauge("rates.requests", rates.requests);

            // counter!(self, snapshot, requests);
            // rate!(self, rates, requests);
        }
    }

    fn update_perf(&self, process_stat: &mut ProcessStat) -> Result<()> {
        let ProcessMemoryInfo {
            resident_set_size,
            virtual_memory_size,
            ..
        } = get_process_memory_info()?;
        // let core_num = processor_numbers()?;
        let cpu_usage = process_stat.cpu()?;
        let fd_num = fd_count_cur()?;
        let IOStats {
            read_bytes: disk_io_read_bytes,
            write_bytes: disk_io_write_bytes,
            ..
        } = get_process_io_stats()
            .map_err(|e| Error::custom(format!("Failed to get process io stats: {}", e)))?;

        // let time_delta = last_log_time.elapsed();

        // let read_delta = disk_io_read_bytes.checked_sub(last_read).unwrap_or_else(|| {
        //     warn!("new io read bytes value is less than previous, new: {disk_io_read_bytes}, previous: {last_read}");
        //     0
        // });
        // let write_delta = disk_io_write_bytes.checked_sub(last_written).unwrap_or_else(|| {
        //     warn!("new io write bytes value is less than previous, new: {disk_io_write_bytes}, previous: {last_written}");
        //     0
        // });

        let counters = self.counters.as_ref();
        counters
            .resident_memory
            .store(resident_set_size, Ordering::Release);
        counters
            .virtual_memory
            .store(virtual_memory_size, Ordering::Release);
        // counters.core_num.store(core_num, Ordering::Release);
        counters.cpu_usage.store(cpu_usage, Ordering::Release);
        counters.fd_num.store(fd_num, Ordering::Release);
        counters
            .disk_io_read_bytes
            .store(disk_io_read_bytes, Ordering::Release);
        counters
            .disk_io_write_bytes
            .store(disk_io_write_bytes, Ordering::Release);
        // counters
        //     .disk_io_read_per_sec
        //     .store(read_delta as f64 / period_secs, Ordering::Release);
        // counters
        //     .disk_io_write_per_sec
        //     .store(write_delta as f64 / period_secs, Ordering::Release);

        Ok(())
    }

    async fn task(self: Arc<Self>) -> Result<()> {
        let mut post_interval = tokio::time::interval(Duration::from_secs(1));
        let mut minute_interval = tokio::time::interval(Duration::from_secs(60));
        // let mut perf_interval = tokio::time::interval(Duration::from_secs(1));

        let mut process_stat = ProcessStat::cur()?;

        let mut snapshot = Snapshot::default();
        let mut instant = Instant::now();

        loop {
            select! {
                _ = post_interval.tick().fuse() => {

                    let previous = snapshot;
                    snapshot = Snapshot::from(&*self.counters);

                    let period = instant.elapsed().as_secs_f64();
                    instant = Instant::now();

                    let rates = Rates::from(&previous, &snapshot, period);

                    if let Err(err) = self.update_perf(&mut process_stat) {
                        error!("Failed to update performance metrics: {err}");
                    }


                    self.post(&snapshot, rates).await;
                }
                _ = minute_interval.tick().fuse() => {
                    self.counters.db_disk_space_bytes.store(self.db.disk_space(), Ordering::Release);
                }
                _ = self.shutdown.request.recv().fuse() => {
                    break;
                }
            }
        }

        self.shutdown.response.send(()).await?;

        Ok(())
    }
}

const SERVICE: &str = "METRICS";

#[async_trait]
impl Service for Metrics {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        let this = self.clone();
        task::spawn(async move {
            this.task()
                .await
                .unwrap_or_else(|err| log_error!("{SERVICE} error: {err}"));
        });

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        self.shutdown.request.try_send(()).unwrap();
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        self.shutdown.response.recv().await?;
        Ok(())
    }
}
