use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Error: {0}")]
    Custom(String),

    #[error(transparent)]
    Core(#[from] krc721_core::error::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Unable to start kaspa daemon: `{0}`")]
    NodeStartupError(std::io::Error),

    #[error("Unable to acquire kaspa daemon stdout handle")]
    NodeStdoutHandleError,
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
    }
}
