use crate::imports::*;
use kaspa_addresses::Address;
use krc721_core::model::kasplex::v1::model::krc20::TokenTransaction as Krc20Operation;
use krc721_core::model::krc721::Operation as Krc721Operation;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Notification {
    Test,
    Address { address: Address },
    Krc20Operation { operation: Box<Krc20Operation> },
    Krc721Operation { operation: Box<Krc721Operation> },
}

impl Serializer for Notification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        match self {
            Notification::Test => {
                store!(u8, &0, writer)?;
            }
            Notification::Address { address } => {
                store!(u8, &1, writer)?;
                store!(Address, &address, writer)?;
            }
            Notification::Krc20Operation { operation } => {
                store!(u8, &2, writer)?;
                store!(Krc20Operation, &operation, writer)?;
            }
            Notification::Krc721Operation { operation } => {
                store!(u8, &3, writer)?;
                store!(Krc721Operation, &operation, writer)?;
            }
        }
        Ok(())
    }
}

impl Deserializer for Notification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let update_type = load!(u8, reader)?;
        match update_type {
            0 => Ok(Notification::Test),
            1 => {
                let address = load!(Address, reader)?;
                Ok(Notification::Address { address })
            }
            2 => {
                let operation = load!(Krc20Operation, reader)?;
                Ok(Notification::Krc20Operation {
                    operation: Box::new(operation),
                })
            }
            3 => {
                let operation = load!(Krc721Operation, reader)?;
                Ok(Notification::Krc721Operation {
                    operation: Box::new(operation),
                })
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid notification type",
            )),
        }
    }
}

// impl Notification {
//     pub fn event_type(&self) -> EventType {
//         match self {
//             Notification::Test => EventType::Test,
//             Notification::Address { .. } => EventType::Address,
//             Notification::Krc20Operation { .. } => EventType::Krc20Operation,
//             Notification::Krc721Operation { .. } => EventType::Krc721Operation,
//         }
//     }
// }

// #[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
// pub enum EventType {
//     Test,
//     Address,
//     Krc20Operation,
//     Krc721Operation,
// }

#[derive(
    Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, BorshSerialize, BorshDeserialize,
)]
pub enum Subscription {
    Test,
    Address { address_list: Option<Vec<Address>> },
    Krc20Operation { address_list: Option<Vec<Address>> },
    Krc721Operation { address_list: Option<Vec<Address>> },
}

impl Serializer for Subscription {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Subscription, &self, writer)?;
        Ok(())
    }
}

impl Deserializer for Subscription {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let subscription = load!(Subscription, reader)?;
        Ok(subscription)
    }
}
