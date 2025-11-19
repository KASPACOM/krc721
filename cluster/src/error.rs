use krc721_rpc_core::imports::NetworkId;
// use std::sync::PoisonError;
use thiserror::Error;
use workflow_core::channel::{ChannelError, RecvError, SendError, TryRecvError, TrySendError};

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    // #[error(transparent)]
    // Serde(#[from] serde_json::Error),
    #[error("Channel send() error")]
    SendError,

    #[error("Channel recv() error")]
    RecvError,

    #[error("Channel try_send() error")]
    TrySendError,

    #[error("Channel try_recv() error")]
    TryRecvError,

    #[error("Channel error: {0}")]
    ChannelError(String),
    // #[error("Poison error -> {0:?}")]
    // PoisonError(String),
    #[error("TOML error: {0}")]
    TomlError(#[from] toml::de::Error),

    // #[error(transparent)]
    // Nexus(#[from] krc721_nexus::error::Error),
    #[error("RPC error: {0}")]
    RpcError(Box<krc721_rpc_client::error::Error>),

    #[error(
        "Network mismatch for URL: `{url}` - expecting: `{expecting}`, connected to: `{actual}`"
    )]
    NetworkMismatch {
        url: String,
        expecting: NetworkId,
        actual: NetworkId,
    },

    #[error("Client negotiation error")]
    ClientNegotiation,
}

impl Error {
    pub fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
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

impl<T> From<ChannelError<T>> for Error {
    fn from(err: ChannelError<T>) -> Self {
        Error::ChannelError(err.to_string())
    }
}

// impl<T> From<PoisonError<T>> for Error {
//     fn from(err: PoisonError<T>) -> Self {
//         Self::PoisonError(format!("{err:?}"))
//     }
// }

impl From<krc721_rpc_client::error::Error> for Error {
    fn from(err: krc721_rpc_client::error::Error) -> Self {
        Error::RpcError(Box::new(err))
    }
}
