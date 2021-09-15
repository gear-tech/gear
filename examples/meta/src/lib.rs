#![no_std]

use codec::{Decode, Encode};
use gstd::{msg, prelude::*};
use gstd_meta::{meta, TypeInfo};

// Metatypes for input and output
#[derive(TypeInfo, Decode)]
pub struct MessageInitIn {
    pub amount: u8,
    pub currency: Vec<u8>,
}

#[derive(TypeInfo, Encode)]
pub struct MessageInitOut {
    pub exchange_rate: Result<u8, u8>,
    pub sum: u8,
}

impl From<MessageInitIn> for MessageInitOut {
    fn from(other: MessageInitIn) -> Self {
        let rate = match String::from_utf8(other.currency)
            .expect("Unable to parse str")
            .as_ref()
        {
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

#[derive(TypeInfo, Decode)]
pub struct MessageIn {
    pub id: Id,
}

#[derive(TypeInfo, Encode)]
pub struct MessageOut {
    pub res: Option<Wallet>,
}

impl From<MessageIn> for MessageOut {
    fn from(other: MessageIn) -> Self {
        unsafe {
            for wallet in WALLETS.iter() {
                if wallet.id.decimal == other.id.decimal {
                    return Self {
                        res: Some(wallet.clone())
                    }
                };
            };
            
            Self {
                res: None
            }
        }
    }
}

// Additional to primary types
#[derive(TypeInfo, Decode, Encode, Debug, PartialEq, Clone)]
pub struct Id {
    pub decimal: u64,
    pub hex: Vec<u8>,
}

#[derive(TypeInfo, Encode, Clone)]
pub struct Person {
    pub surname: Vec<u8>,
    pub name: Vec<u8>,
}

#[derive(TypeInfo, Encode, Clone)]
pub struct Wallet {
    pub id: Id,
    pub person: Person,
}

meta! {
    title: "Example program with metadata",
    input: MessageIn,
    output: MessageOut,
    init_input: MessageInitIn,
    init_output: MessageInitOut,
    extra: Id, Person, Wallet
}

static mut WALLETS: Vec<Wallet> = Vec::new();

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let message_in: MessageIn = msg::load().unwrap();
    let message_out = MessageOut::from(message_in);

    msg::reply(message_out, 0, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    WALLETS.push(Wallet {
        id: Id {
            decimal: 1,
            hex: vec![1u8],
        },
        person: Person {
            surname: "SomeName".into(),
            name: "SomeSurname".into(),
        },
    });
    WALLETS.push(Wallet {
        id: Id {
            decimal: 2,
            hex: vec![2u8],
        },
        person: Person {
            surname: "OtherName".into(),
            name: "OtherSurname".into(),
        },
    });

    let message_init_in: MessageInitIn = msg::load().unwrap();
    let message_init_out = MessageInitOut::from(message_init_in);

    msg::send(0.into(), message_init_out, 0);
}
