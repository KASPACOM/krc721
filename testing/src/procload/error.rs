use thiserror::Error;


#[derive(Error, Debug)]
pub enum Error {
    #[error("Error: {0}")]
    Custom(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ParseInt")]
    ParseInt(#[from] std::num::ParseIntError),

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

    #[error("Invalid slice")]
    TryFromSlice(#[from] std::array::TryFromSliceError),

    #[error("Invalid JSON: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("API Version `{0}` is not supported")]
    ApiVersionNotSupported(u32),

    #[error("Invalid network id : {0}")]
    NetworkId(String),

    #[error("Service not available")]
    ServiceNotAvailable,

}
