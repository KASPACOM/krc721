use crate::config::NodeConfig;
use crate::imports::*;
use krc721_core::model::krc721::IndexerStatus;
use std::time::Duration;
use tracing::*;

const DEFAULT_POLL_RATE: u64 = 5;

pub struct ConnRef<'c> {
    pub connection: &'c Connection,
}

impl<'c> ConnRef<'c> {
    pub fn new(connection: &'c Connection) -> Self {
        connection.acquire();

        Self { connection }
    }
}

impl Drop for ConnRef<'_> {
    fn drop(&mut self) {
        self.connection.release();
    }
}

impl AsRef<Krc721RpcClient> for ConnRef<'_> {
    fn as_ref(&self) -> &Krc721RpcClient {
        self.connection.rpc()
    }
}

struct Inner {
    network_id: NetworkId,
    config: NodeConfig,
    shutdown: DuplexChannel<()>,
    rpc: Krc721RpcClient,
    is_connected: AtomicBool,
    sessions: AtomicU64,
    // ---
    is_node_connected: AtomicBool,
    is_node_synced: AtomicBool,
    is_indexer_synced: AtomicBool,
    // ---
    is_available: AtomicBool,
}

#[derive(Clone)]
pub struct Connection {
    #[allow(unused)]
    inner: Arc<Inner>,
}

impl Connection {
    pub fn new(network_id: NetworkId, config: NodeConfig) -> Self {
        let rpc = Krc721RpcClient::try_new(&config.url, None).unwrap();
        Self {
            inner: Arc::new(Inner {
                network_id,
                config,
                rpc,
                is_connected: AtomicBool::new(false),
                sessions: AtomicU64::new(0),
                shutdown: DuplexChannel::oneshot(),
                is_node_connected: AtomicBool::new(false),
                is_node_synced: AtomicBool::new(false),
                is_indexer_synced: AtomicBool::new(false),
                is_available: AtomicBool::new(false),
            }),
        }
    }

    pub fn url(&self) -> &str {
        &self.inner.config.url
    }

    #[inline(always)]
    pub fn rpc(&self) -> &Krc721RpcClient {
        &self.inner.rpc
    }

    #[inline(always)]
    pub fn is_available(&self) -> bool {
        self.inner.is_available.load(Ordering::SeqCst)
    }

    #[inline(always)]
    pub fn is_connected(&self) -> bool {
        self.inner.is_connected.load(Ordering::SeqCst)
    }

    #[inline(always)]
    pub fn is_indexer_synced(&self) -> bool {
        self.inner.is_indexer_synced.load(Ordering::SeqCst)
    }

    #[inline(always)]
    pub fn is_node_synced(&self) -> bool {
        self.inner.is_node_synced.load(Ordering::SeqCst)
    }

    #[inline(always)]
    pub fn is_node_connected(&self) -> bool {
        self.inner.is_node_connected.load(Ordering::SeqCst)
    }

    #[inline(always)]
    pub fn sessions(&self) -> u64 {
        (self.inner.sessions.load(Ordering::SeqCst) as f64 * self.inner.config.bias.unwrap_or(1.0))
            .floor() as u64
    }

    #[inline(always)]
    pub fn acquire(&self) -> ConnRef<'_> {
        self.inner.sessions.fetch_add(1, Ordering::Relaxed);
        ConnRef {
            connection: self,
            // connection: self.clone(),
        }
    }

    #[inline(always)]
    pub fn release(&self) {
        self.inner.sessions.fetch_sub(1, Ordering::Relaxed);
    }

    pub async fn handle_connect(&self) -> Result<()> {
        info!("Connected to '{}'", self.url());

        let GetStatusResponse { response, .. } = self.rpc().get_status().await?;

        let IndexerStatus {
            network,
            is_node_connected,
            is_node_synced,
            is_indexer_synced,
            ..
        } = &*response;

        let network_id = NetworkId::from(*network);
        if network_id != self.inner.network_id {
            return Err(Error::NetworkMismatch {
                url: self.url().to_owned(),
                expecting: self.inner.network_id,
                actual: network_id,
            });
        }

        self.inner
            .is_node_connected
            .store(*is_node_connected, Ordering::SeqCst);
        self.inner
            .is_indexer_synced
            .store(*is_indexer_synced, Ordering::SeqCst);
        self.inner
            .is_node_synced
            .store(*is_node_synced, Ordering::SeqCst);

        info!(
            "'{}' node online: {} node synced: {} indexer synced: {}",
            self.url(),
            *is_node_connected,
            *is_node_synced,
            *is_indexer_synced
        );

        self.inner.is_connected.store(true, Ordering::SeqCst);

        let is_available = *is_node_connected && *is_node_synced && *is_indexer_synced;
        self.inner
            .is_available
            .store(is_available, Ordering::SeqCst);

        Ok(())
    }

    pub async fn handle_disconnect(&self) -> Result<()> {
        self.inner.is_available.store(false, Ordering::SeqCst);
        self.inner.is_connected.store(false, Ordering::SeqCst);
        // ---
        self.inner.is_node_connected.store(false, Ordering::SeqCst);
        self.inner.is_indexer_synced.store(false, Ordering::SeqCst);

        warn!("Disconnected from '{}'", self.url());

        Ok(())
    }

    pub async fn poll(&self) -> Result<()> {
        let GetSyncStatusResponse {
            is_node_connected,
            is_node_synced,
            is_indexer_synced,
        } = self.rpc().get_sync_status().await?;

        let is_available = is_node_connected && is_node_synced && is_indexer_synced;
        self.inner
            .is_available
            .store(is_available, Ordering::SeqCst);

        self.inner
            .is_node_connected
            .store(is_node_connected, Ordering::SeqCst);
        self.inner
            .is_node_synced
            .store(is_node_synced, Ordering::SeqCst);
        self.inner
            .is_indexer_synced
            .store(is_indexer_synced, Ordering::SeqCst);

        Ok(())
    }

    pub async fn task(&self) -> Result<()> {
        let rpc_ctl_channel = self.rpc().ctl_multiplexer().channel();
        // let notification_receiver = self.inner.notification_channel.receiver.clone();
        let mut poller = tokio::time::interval(Duration::from_secs(
            self.inner.config.poll_rate.unwrap_or(DEFAULT_POLL_RATE),
        ));

        self.rpc().connect_as_task()?;

        loop {
            select_biased! {
                msg = rpc_ctl_channel.receiver.recv().fuse() => {
                    match msg {
                        Ok(msg) => {
                            // handle RPC channel connection and disconnection events
                            match msg {
                                WrpcCtl::Connect => {
                                    if !self.is_connected() {
                                        if let Err(err) = self.handle_connect().await {
                                            error!("Connection failure on '{}' - {err}", self.url());
                                        }
                                    } else {
                                        error!("Connection to indexer is already established");
                                    }
                                },
                                WrpcCtl::Disconnect => {
                                    if self.is_connected() {
                                        if let Err(err) = self.handle_disconnect().await {
                                            error!("Disconnect task error on '{}': {err}", self.url());
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            panic!("Cluster Krc721RpcClient channel error: {err}");
                        }
                    }
                }
                // notification = notification_receiver.recv().fuse() => {
                //     match notification {
                //         Ok(notification) => {
                //             if let Err(err) = self.handle_notification(notification).await {
                //                 log_error!("error while handling notification: {err}");
                //             }
                //         }
                //         Err(err) => {
                //             panic!("RPC notification channel error: {err}");
                //         }
                //     }
                // },

                _ = poller.tick().fuse() => {
                    let _ = self.poll().await;
                }

                // we use select_biased to drain rpc_ctl
                // and notifications before shutting down
                // as such task_ctl is last in the poll order
                _ = self.inner.shutdown.request.recv().fuse() => {
                    break;
                },

            }
        }

        // handle power down on rpc channel that remains connected
        if self.is_connected() {
            self.rpc()
                .disconnect()
                .await
                .unwrap_or_else(|err| error!("{err}"));
            self.handle_disconnect()
                .await
                .unwrap_or_else(|err| error!("{err}"));
        }

        self.inner.shutdown.response.send(()).await?;

        Ok(())
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.request.try_send(()).unwrap();
    }

    pub async fn join(&self) -> ServiceResult<()> {
        self.inner.shutdown.response.recv().await?;
        Ok(())
    }
}
