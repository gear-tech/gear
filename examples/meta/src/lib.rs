#![no_std]

use codec::{Decode, Encode};
use gstd::{msg, prelude::*};
use scale_info::TypeInfo;

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
            let res = WALLETS
                .iter()
                .find(|w| w.id.decimal == other.id.decimal)
                .map(Clone::clone);

            Self { res }
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

gstd::metadata! {
    title: "Example program with metadata",
    init:
        input: MessageInitIn,
        output: MessageInitOut,
    handle:
        input: MessageIn,
        output: MessageOut
}

static mut WALLETS: Vec<Wallet> = Vec::new();

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let message_in: MessageIn = msg::load().unwrap();
    let message_out: MessageOut = message_in.into();

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
            surname: "SomeSurname".into(),
            name: "SomeName".into(),
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
    let message_init_out: MessageInitOut = message_init_in.into();

    msg::send(0.into(), message_init_out, 0, 0);
}
