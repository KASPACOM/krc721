use crate::imports::*;
use crate::progress::Progress;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default)]
pub enum PhaseContext {
    #[default]
    None,
    Archiving {
        progress: Arc<Progress>,
    },
    Restoring {
        progress: Arc<Progress>,
    },
    Ready {
        archive: PathBuf,
        daa_score: u64,
    },
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub enum Phase {
    #[default]
    None,
    Archiving {
        progress: f64,
    },
    Restoring {
        progress: f64,
    },
    Ready {
        archive: String,
        daa_score: u64,
    },
}

impl From<PhaseContext> for Phase {
    fn from(context: PhaseContext) -> Self {
        match context {
            PhaseContext::Archiving { progress } => Self::Archiving {
                progress: progress.progress(),
            },
            PhaseContext::Restoring { progress } => Self::Restoring {
                progress: progress.progress(),
            },
            PhaseContext::Ready { archive, daa_score } => Self::Ready {
                archive: archive.display().to_string(),
                daa_score,
            },
            _ => Self::None,
        }
    }
}
