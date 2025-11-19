use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    RecvError(#[from] crossbeam_channel::RecvError),
    #[error("Channel send() error")]
    SendError,
    #[error(transparent)]
    Db(#[from] krc721_database::error::Error),
    #[error("Unexpected data from source")]
    UnexpectedKaspaNodeBehaviour,
}
