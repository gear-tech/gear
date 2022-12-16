use crate::{MessageIn, MessageInitIn, MessageInitOut, MessageOut, Wallet};
use gstd::{msg, prelude::*};

// State
static mut WALLETS: Vec<Wallet> = Vec::new();

// Init function
#[no_mangle]
extern "C" fn init() {
    unsafe { WALLETS = Wallet::test_sequence() };

    if msg::size() == 0 {
        return;
    }

    let message_init_in: MessageInitIn = msg::load().unwrap();
    let message_init_out: MessageInitOut = message_init_in.into();

    msg::reply(message_init_out, 0).unwrap();
}

// Handle function
#[no_mangle]
extern "C" fn handle() {
    let message_in: MessageIn = msg::load().unwrap();

    let res = unsafe { &WALLETS }
        .iter()
        .find(|w| w.id.decimal == message_in.id.decimal)
        .map(Clone::clone);

    let message_out = MessageOut { res };

    msg::reply(message_out, 0).unwrap();
}

// State-sharing function
#[no_mangle]
extern "C" fn state() {
    msg::reply(unsafe { WALLETS.clone() }, 0).expect("Failed to share state");
}

// Hash of metadata sharing function (to on-chain verify metadata compatibility)
#[no_mangle]
extern "C" fn metahash() {
    let metahash: [u8; 32] = include!("../.metahash");
    msg::reply(metahash, 0).expect("Failed to share metahash");
}
