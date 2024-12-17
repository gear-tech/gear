use crate::{MessageIn, MessageInitIn, MessageInitOut, MessageOut, Wallet};
use gstd::{msg, prelude::*};

// State
static mut WALLETS: Vec<Wallet> = Vec::new();

// Init function
#[unsafe(no_mangle)]
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
#[unsafe(no_mangle)]
extern "C" fn handle() {
    let message_in: MessageIn = msg::load().unwrap();

    let res = unsafe { static_ref!(WALLETS) }
        .iter()
        .find(|w| w.id.decimal == message_in.id.decimal)
        .cloned();

    let message_out = MessageOut { res };

    msg::reply(message_out, 0).unwrap();
}

// State-sharing function
#[unsafe(no_mangle)]
extern "C" fn state() {
    msg::reply(unsafe { static_ref!(WALLETS).clone() }, 0).expect("Failed to share state");
}
