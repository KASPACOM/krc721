use kaspa_wrpc_client::prelude::RpcError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("User abort")]
    UserAbort,

    #[error(transparent)]
    HttpError(#[from] workflow_http::error::Error),

    #[error(transparent)]
    Core(#[from] krc721_core::error::Error),

    #[error("{0}")]
    Krc721RpcClient(Box<krc721_rpc_client::error::Error>),

    #[error("{0}")]
    KaspaWrpcClient(Box<kaspa_wrpc_client::error::Error>),

    #[error("{0}")]
    Wallet(Box<kaspa_wallet_core::error::Error>),

    #[error("{0}")]
    KaspaRpcClient(Box<kaspa_wrpc_client::error::Error>),

    #[error("Indexer error: {0}")]
    IndexerError(String),

    #[error("Listener error: {0}")]
    ListenerError(String),

    #[error("Shutdown receiver error: {0}")]
    ShutdownReceiverError(String),

    #[error("{0}")]
    KaspaRpc(Box<RpcError>),
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
    }
}

impl From<krc721_rpc_client::error::Error> for Error {
    fn from(err: krc721_rpc_client::error::Error) -> Self {
        Error::Krc721RpcClient(Box::new(err))
    }
}

impl From<kaspa_wrpc_client::error::Error> for Error {
    fn from(err: kaspa_wrpc_client::error::Error) -> Self {
        Error::KaspaWrpcClient(Box::new(err))
    }
}

impl From<RpcError> for Error {
    fn from(err: RpcError) -> Self {
        Error::KaspaRpc(Box::new(err))
    }
}

impl From<kaspa_wallet_core::error::Error> for Error {
    fn from(err: kaspa_wallet_core::error::Error) -> Self {
        Error::Wallet(Box::new(err))
    }
}
