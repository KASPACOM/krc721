use crate::imports::*;

#[derive(Default)]
pub struct State {
    is_node_connected: AtomicBool,
    is_node_synced: AtomicBool,
    current_daa_score: AtomicU64,
}

impl State {
    /// Signifies **valid and negotiated** connection to the node.
    /// This flag is set to true only after the connection is established
    /// and the node is validated for the required features / state.
    pub fn is_node_connected(&self) -> bool {
        self.is_node_connected.load(Ordering::SeqCst)
    }

    pub fn is_node_synced(&self) -> bool {
        self.is_node_synced.load(Ordering::SeqCst)
    }

    pub(crate) fn set_is_node_connected(&self, value: bool) {
        self.is_node_connected.store(value, Ordering::SeqCst);
    }

    pub(crate) fn set_is_node_synced(&self, value: bool) {
        self.is_node_synced.store(value, Ordering::SeqCst);
    }

    pub(crate) fn set_current_daa_score(&self, value: u64) {
        self.current_daa_score.store(value, Ordering::SeqCst);
    }

    pub fn current_daa_score(&self) -> u64 {
        self.current_daa_score.load(Ordering::SeqCst)
    }
}
