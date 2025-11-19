use crate::imports::*;
use crate::router::Router;
use crate::{connection::*, server::*};
use krc721_core::runtime::*;
use krc721_nexus::prelude::Nexus;
use krc721_rpc_core::prelude::*;
use tracing::log::warn;
use tracing::Instrument;
pub use workflow_rpc::server::{Encoding as WrpcEncoding, WebSocketConfig, WebSocketCounters};

static MAX_WRPC_MESSAGE_SIZE: usize = 1024 * 1024 * 128; // 128MB

/// Options for configuring the wRPC server
pub struct WrpcOptions {
    pub listen_address: String,
    pub verbose: bool,
    pub encoding: WrpcEncoding,
}

impl Default for WrpcOptions {
    fn default() -> Self {
        WrpcOptions {
            listen_address: "localhost:7878".to_owned(),
            verbose: false,
            encoding: WrpcEncoding::Borsh,
        }
    }
}

impl WrpcOptions {
    pub fn listen(mut self, address: &str) -> Self {
        address.clone_into(&mut self.listen_address);
        self
    }
}

pub struct WrpcService {
    options: Arc<WrpcOptions>,
    rpc_server: RpcServer,
    // server: Server,
    shutdown: Channel<()>,
}

impl WrpcService {
    /// Create and initialize RpcServer
    pub async fn try_new(
        nexus: &Nexus,
        options: WrpcOptions,
        // counters: Arc<WebSocketCounters>,
    ) -> Result<Self> {
        let options = Arc::new(options);
        // Create handle to manage connections
        // let server = Arc::new(Server::new(
        let server = Server::new(
            nexus,
            options.clone(),
            // *encoding,
            // handler,
        );

        // Create router (initializes Interface registering RPC method and notification handlers)
        let router = Arc::new(Router::new(server.clone()));
        // Create a server
        let rpc_server = RpcServer::new_with_encoding::<Server, Connection, RpcApiOps, Id64>(
            options.encoding,
            Arc::new(server.clone()),
            router.interface.clone(),
            None,
            // Some(counters),
            true,
        );

        Ok(WrpcService {
            options,
            // server,
            rpc_server,
            shutdown: Channel::oneshot(),
        })
    }
}

#[async_trait]
impl Service for WrpcService {
    async fn spawn(self: Arc<Self>, _runtime: Runtime) -> ServiceResult<()> {
        let listen_address = self.options.listen_address.clone();
        tracing::info!("wRPC server listening on: {}", listen_address);
        let listener = self
            .rpc_server
            .bind(listen_address.as_str())
            .await
            .map_err(ServiceError::custom)?;
        let span = tracing::Span::current();
        spawn(
            async move {
                let config = WebSocketConfig {
                    max_message_size: Some(MAX_WRPC_MESSAGE_SIZE),
                    ..Default::default()
                };
                let serve_result = self.rpc_server.listen(listener, Some(config)).await;
                match serve_result {
                    Ok(_) => tracing::info!("wRPC Server stopped on: {}", listen_address),
                    Err(err) => panic!("wRPC Server {listen_address} stopped with error: {err:?}"),
                }
            }
            .instrument(span),
        );

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        spawn(async move {
            _ = self
                .rpc_server
                .stop()
                .inspect_err(|err| warn!("wRPC unable to signal shutdown: `{err}`"));
            _ = self
                .rpc_server
                .join()
                .await
                .inspect_err(|err| warn!("wRPC error: `{err}"));

            self.shutdown.send(()).await.unwrap();
        });
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        self.shutdown.recv().await?;
        Ok(())
    }
}
