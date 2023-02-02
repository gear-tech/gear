#![no_std]

extern crate alloc;

use alloc::{string::String, vec, vec::Vec};
use codec::{Decode, Encode};
use gmeta::{InOut, Metadata};
use scale_info::TypeInfo;

pub struct ProgramMetadata;

impl Metadata for ProgramMetadata {
    type Init = InOut<MessageInitIn, MessageInitOut>;
    type Handle = InOut<MessageIn, MessageOut>;
    type Others = InOut<MessageAsyncIn, Option<u8>>;
    type Reply = InOut<String, Vec<u16>>;
    type Signal = ();
    type State = Vec<Wallet>;
}

// Metatypes for input and output
#[derive(TypeInfo, Default, Decode, Encode)]
pub struct MessageInitIn {
    pub amount: u8,
    pub currency: String,
}

#[derive(TypeInfo, Decode, Encode)]
pub struct MessageInitOut {
    pub exchange_rate: Result<u8, u8>,
    pub sum: u8,
}

impl From<MessageInitIn> for MessageInitOut {
    fn from(other: MessageInitIn) -> Self {
        let rate = match other.currency.as_ref() {
            "USD" => Ok(2),
            "EUR" => Ok(3),
            _ => Err(1),
        };

        Self {
            exchange_rate: rate,
            sum: rate.unwrap_or(0) * other.amount,
        }
    }
}

#[derive(TypeInfo, Decode, Encode)]
pub struct MessageIn {
    pub id: Id,
}

#[derive(TypeInfo, Decode, Encode)]
pub struct MessageOut {
    pub res: Option<Wallet>,
}

// Additional to primary types
#[derive(TypeInfo, Decode, Encode, Debug, PartialEq, Eq, Clone)]
pub struct Id {
    pub decimal: u64,
    pub hex: Vec<u8>,
}

#[derive(TypeInfo, Decode, Encode, Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub surname: String,
    pub name: String,
}

#[derive(TypeInfo, Decode, Encode, Clone, Debug, PartialEq)]
pub struct Wallet {
    pub id: Id,
    pub person: Person,
}

impl Wallet {
    pub fn test_sequence() -> Vec<Self> {
        vec![
            Wallet {
                id: Id {
                    decimal: 1,
                    hex: [1].to_vec(),
                },
                person: Person {
                    surname: "SomeSurname".into(),
                    name: "SomeName".into(),
                },
            },
            Wallet {
                id: Id {
                    decimal: 2,
                    hex: [2].to_vec(),
                },
                person: Person {
                    surname: "OtherSurname".into(),
                    name: "OtherName".into(),
                },
            },
        ]
    }
}

#[derive(TypeInfo, Decode, Encode, Clone)]
pub struct MessageAsyncIn {
    pub empty: (),
}

#[derive(TypeInfo, Encode, Decode, Clone)]
pub struct MessageAsyncOut {
    pub empty: (),
}
