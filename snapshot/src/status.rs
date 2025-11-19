use crate::imports::*;
use crate::phase::Phase;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub daa_score: u64,
    pub sync: bool,
    pub phase: Phase,
}
