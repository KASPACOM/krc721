use thiserror::Error;

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

    #[error(transparent)]
    RpcCore(#[from] krc721_rpc_core::error::Error),

    #[error("{0}")]
    RpcServer(Box<krc721_rpc_server::error::Error>),

    #[error(transparent)]
    Database(#[from] krc721_database::error::Error),

    #[error(transparent)]
    Snapshot(#[from] krc721_snapshot::error::Error),

    #[error(transparent)]
    Kaspad(#[from] krc721_kaspad::error::Error),

    #[error("{0}")]
    Wrpc(Box<kaspa_wrpc_client::error::Error>),

    #[error("{0}")]
    Cluster(Box<krc721_cluster::error::Error>),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error(transparent)]
    FasterHex(#[from] faster_hex::Error),

    #[error(transparent)]
    TickError(#[from] krc721_core::error::TickError),

    #[error(transparent)]
    AddressError(#[from] kaspa_addresses::AddressError),
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
    }
}

impl From<krc721_rpc_server::error::Error> for Error {
    fn from(err: krc721_rpc_server::error::Error) -> Self {
        Error::RpcServer(Box::new(err))
    }
}

impl From<kaspa_wrpc_client::error::Error> for Error {
    fn from(err: kaspa_wrpc_client::error::Error) -> Self {
        Error::Wrpc(Box::new(err))
    }
}

impl From<krc721_cluster::error::Error> for Error {
    fn from(err: krc721_cluster::error::Error) -> Self {
        Error::Cluster(Box::new(err))
    }
}
