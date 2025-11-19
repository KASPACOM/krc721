use crate::imports::*;

pub trait ContextT: Send + Sync {
    fn id(&self) -> u64;
    fn notify(&self, notification: &Notification) -> Result<()>;
}
