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
        let rate: Result<u8, u8> = match String::from_utf8(other.currency)
            .expect("Unable to parse str")
            .as_ref()
        {
            "USD" => Ok(75),
            "EUR" => Ok(90),
            _ => Err(1),
        };

        MessageInitOut {
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
    pub res: Vec<Result<Wallet, Vec<u8>>>,
}

impl From<MessageIn> for MessageOut {
    fn from(other: MessageIn) -> Self {
        unsafe {
            let wallet: Vec<Wallet> = WALLETS
                .clone()
                .into_iter()
                .filter(|v| v.id.decimal == other.id.decimal)
                .collect();
            if wallet.is_empty() {
                MessageOut {
                    res: vec![Err("404 not_found".as_bytes().into())],
                }
            } else {
                MessageOut {
                    res: vec![Ok(wallet[0].clone())],
                }
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
    pub patronymic: Option<Vec<u8>>,
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
            hex: [1].into(),
        },
        person: Person {
            surname: "SomeName".to_string().into_bytes(),
            name: "SomeSurname".to_string().into_bytes(),
            patronymic: None,
        },
    });
    WALLETS.push(Wallet {
        id: Id {
            decimal: 2,
            hex: [2].into(),
        },
        person: Person {
            surname: "OtherName".to_string().into_bytes(),
            name: "OtherSurname".to_string().into_bytes(),
            patronymic: Some("OtherPatronymic".to_string().into_bytes()),
        },
    });

    let message_init_in: MessageInitIn = msg::load().unwrap();
    let message_init_out = MessageInitOut::from(message_init_in);

    msg::send(0.into(), message_init_out, 0);
}
