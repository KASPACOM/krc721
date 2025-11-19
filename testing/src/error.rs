use thiserror::Error;
use workflow_core::channel::{RecvError, SendError, TryRecvError, TrySendError};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Error: {0}")]
    Custom(String),

    #[error(transparent)]
    Core(#[from] krc721_core::error::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error(transparent)]
    Nexus(#[from] krc721_nexus::error::Error),

    #[error(transparent)]
    Http(#[from] krc721_http_server::error::Error),

    // #[error(transparent)]
    // RpcCore(#[from] krc721_rpc_core::error::Error),

    // #[error(transparent)]
    // RpcServer(#[from] krc721_rpc_server::error::Error),
    #[error(transparent)]
    Database(#[from] krc721_database::error::Error),

    // #[error(transparent)]
    // Kaspad(#[from] krc721_kaspad::error::Error),
    #[error(transparent)]
    Wrpc(#[from] kaspa_wrpc_client::error::Error),

    #[error("Channel send() error")]
    SendError,

    #[error("Channel recv() error")]
    RecvError,

    #[error("Channel try_send() error")]
    TrySendError,

    #[error("Channel try_recv() error")]
    TryRecvError,
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
    }
}

impl<T> From<SendError<T>> for Error {
    fn from(_: SendError<T>) -> Self {
        Error::SendError
    }
}

impl<T> From<TrySendError<T>> for Error {
    fn from(_: TrySendError<T>) -> Self {
        Error::TrySendError
    }
}

impl From<RecvError> for Error {
    fn from(_: RecvError) -> Self {
        Error::RecvError
    }
}

impl From<TryRecvError> for Error {
    fn from(_: TryRecvError) -> Self {
        Error::TryRecvError
    }
}
