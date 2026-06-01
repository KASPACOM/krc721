use crate::imports::*;

pub trait ConsumerT: Send + Sync + 'static {
    fn handle_virtual_chain_changed(
        &self,
        notification: VirtualChainChangedNotification,
    ) -> Result<()>;

    fn disconnected(self: Arc<Self>) -> Result<()> {
        Ok(())
    }
}
