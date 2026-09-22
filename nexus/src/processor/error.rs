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
    #[error("Historical application failed: {0}")]
    HistoricalApplication(String),
    #[error("cannot rewind an empty database")]
    NoAcceptedBlockForRewind,
    #[error("rewind blue score {requested} is beyond current tip {tip}")]
    RewindBeyondTip { requested: u64, tip: u64 },
    #[error("rewind blue score {requested} would remove every retained accepted block")]
    RewindWouldRemoveAllAcceptedBlocks { requested: u64 },
    #[error("blue score {0} is too large to convert to an operation score")]
    BlueScoreOverflow(u64),
    #[error("database write conflict while committing rewind")]
    RewindWriteConflict,
    #[error("repair source operation is not a transfer")]
    RepairNotTransfer,
    #[error("repair transaction {0} already exists in target database")]
    RepairAlreadyExists(String),
    #[error(
        "repair operation score {repair_score} is not newer than token state score {current_score}"
    )]
    RepairWouldRewriteLaterState {
        repair_score: u64,
        current_score: u64,
    },
    #[error("repair transfer failed target-state validation: {0}")]
    RepairValidation(String),
    #[error("database write conflict while committing transfer repair")]
    RepairWriteConflict,
}
