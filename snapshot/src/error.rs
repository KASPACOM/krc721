use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Borsh error in `{0}` at {1}:{2} - {3}")]
    Borsh(&'static str, &'static str, u32, std::io::Error),

    #[error("Fjall error: {0}")]
    Fjall(#[from] fjall::Error),

    #[error("Fjall LSM error: {0}")]
    Lsm(#[from] fjall::LsmError),

    #[error("Error: {0}")]
    Custom(String),

    #[error("Fs IO error: {0}")]
    FsIo(#[from] std::io::Error),

    #[error("Tokio error: {0}")]
    Tokio(#[from] tokio::task::JoinError),

    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

impl Error {
    pub fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}
