use crate::{Id, MessageIn, MessageInitIn, MessageInitOut, MessageOut, Person, Wallet, WALLETS};
use gstd::{msg, prelude::*};

#[no_mangle]
unsafe extern "C" fn handle() {
    let message_in: MessageIn = msg::load().unwrap();
    let message_out: MessageOut = message_in.into();

    msg::reply(message_out, 0).unwrap();
}

#[no_mangle]
unsafe extern "C" fn init() {
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

    msg::reply(message_init_out, 0).unwrap();
}

#[no_mangle]
unsafe extern "C" fn meta_state() -> *mut [i32; 2] {
    let person: Option<Id> = msg::load().expect("failed to decode input argument");
    let encoded = match person {
        None => WALLETS.encode(),
        Some(lookup_id) => WALLETS
            .iter()
            .filter(|w| w.id == lookup_id)
            .cloned()
            .collect::<Vec<Wallet>>()
            .encode(),
    };
    gstd::util::to_leak_ptr(encoded)
}
