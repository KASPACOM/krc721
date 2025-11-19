use crate::imports::*;
use krc721_nexus::context::ContextT;
use krc721_nexus::error::Error as NexusError;
use krc721_nexus::result::Result as NexusResult;
use std::fmt;
use workflow_serializer::prelude::*;

#[derive(Debug)]
struct ConnectionInner {
    pub id: u64,
    pub peer: SocketAddr,
    pub messenger: Arc<Messenger>,
}

impl ConnectionInner {
    // fn send(&self, message: Message) -> crate::result::Result<()> {
    //     Ok(self.messenger.send_raw_message(message)?)
    // }
}

// impl Notify<Notification> for ConnectionInner {
//     fn notify(&self, notification: Notification) -> NotifyResult<()> {
//         self.send(Connection::into_message(&notification, &self.messenger.encoding().into()))
//             .map_err(|err| NotifyError::General(err.to_string()))
//     }
// }

#[derive(Debug, Clone)]
pub struct Connection {
    inner: Arc<ConnectionInner>,
}

impl Connection {
    pub fn new(id: u64, peer: &SocketAddr, messenger: Arc<Messenger>) -> Connection {
        Connection {
            inner: Arc::new(ConnectionInner {
                id,
                peer: *peer,
                messenger,
            }),
        }
    }

    pub fn ctx(&self) -> Arc<dyn ContextT> {
        self.inner.clone()
    }

    /// Obtain the connection id
    pub fn id(&self) -> u64 {
        self.inner.id
    }

    /// Get a reference to the connection [`Messenger`]
    pub fn messenger(&self) -> &Arc<Messenger> {
        &self.inner.messenger
    }

    pub fn peer(&self) -> &SocketAddr {
        &self.inner.peer
    }

    /// Creates a WebSocket [`Message`] that can be posted to the connection ([`Messenger`]) sink
    /// directly.
    #[allow(clippy::result_large_err)]
    pub fn create_serialized_notification_message<Ops, Msg>(
        encoding: Encoding,
        op: Ops,
        msg: Msg,
    ) -> WrpcResult<Message>
    where
        Ops: OpsT,
        Msg: MsgT,
    {
        match encoding {
            Encoding::Borsh => {
                workflow_rpc::server::protocol::borsh::create_serialized_notification_message(
                    op, msg,
                )
            }
            Encoding::SerdeJson => {
                workflow_rpc::server::protocol::borsh::create_serialized_notification_message(
                    op, msg,
                )
            }
        }
    }
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.inner.id, self.inner.peer)
    }
}

// use workflow_serializer::serializer::SerializerT;

// #[derive(Debug)]
// #[repr(transparent)]
// pub struct Payload<'p, T>(pub &'p T)
// where
//     T: SerializerT;

// impl<'p, T> Payload<'p, T>
// where
//     T: SerializerT,
// {
//     // pub fn into_inner(self) -> T {
//     //     self.0
//     // }
// }

// impl<'p, T> BorshSerialize for Payload<'p, T>
// where
//     T: SerializerT,
// {
//     fn serialize<W: std::io::Write>(&self, target: &mut W) -> std::io::Result<()> {
//         payload::ser::Payload(self.0).serialize(target)?;
//         Ok(())
//     }
// }

// impl<'p, T> BorshDeserialize for Payload<'p, T>
// where
//     T: SerializerT,
// {
//     fn deserialize_reader<R: borsh::io::Read>(_source: &mut R) -> std::io::Result<Self> {
//         unimplemented!()
//     }
// }

// impl<'p, T> Serialize for Payload<'p, T>
// where
//     T: SerializerT,
// {
//     fn serialize<S: serde::Serializer>(
//         &self,
//         serializer: S,
//     ) -> std::result::Result<S::Ok, S::Error> {
//         unimplemented!()
//         // serializer.serialize_str(&format!("{:?}", self.0))
//     }
// }

// impl<'p, 'de, T> Deserialize<'de> for Payload<'p, T>
// where
//     T: SerializerT,
// {
//     fn deserialize<D: serde::Deserializer<'de>>(
//         deserializer: D,
//     ) -> std::result::Result<Self, D::Error> {
//         unimplemented!()
//     }
// }

// #[async_trait::async_trait]
impl ContextT for ConnectionInner {
    fn id(&self) -> u64 {
        self.id
    }

    fn notify(&self, notification: &Notification) -> NexusResult<()> {
        // let payload = payload::ser::Payload(&notification);

        let message = Connection::create_serialized_notification_message(
            self.messenger.encoding(),
            RpcApiOps::Notify,
            // Payload(notification),
            Serializable(notification.clone()),
            // payload,
        )
        .map_err(NexusError::custom)?;

        self.messenger
            .send_raw_message(message)
            .map_err(NexusError::custom)?;

        Ok(())
    }
}
// #[async_trait::async_trait]
// impl ConnectionT for Connection {
//     type Notification = Notification;
//     type Message = Message;
//     type Encoding = NotifyEncoding;
//     type Error = krc721_notify::error::Error;

//     fn encoding(&self) -> Self::Encoding {
//         self.messenger().encoding().into()
//     }

//     fn into_message(notification: &Self::Notification, encoding: &Self::Encoding) -> Self::Message {
//         let op: RpcApiOps = notification.event_type().into();
//         Self::create_serialized_notification_message(encoding.clone().into(), op, notification.clone()).unwrap()
//     }

//     async fn send(&self, message: Self::Message) -> core::result::Result<(), Self::Error> {
//         self.inner.send(message).map_err(|err| NotifyError::General(err.to_string()))
//     }

//     fn close(&self) -> bool {
//         if !self.is_closed() {
//             if let Err(err) = self.messenger().close() {
//                 log_trace!("Error closing connection {}: {}", self.peer(), err);
//             } else {
//                 return true;
//             }
//         }
//         false
//     }

//     fn is_closed(&self) -> bool {
//         self.messenger().sink().is_closed()
//     }
// }

// pub type ConnectionReference = Arc<Connection>;
