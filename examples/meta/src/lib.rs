#![no_std]
#![feature(default_alloc_error_handler)]

// TODO: Deal with `panic_handler`s conflict of `serde` and `no_std` contracts

use gstd::{ext, msg, prelude::*};
use gstd_meta::*;

static mut CURRENT_VALUE: u64 = 0;

// ERR: can't find crate for `serde`
#[derive(Serialize)]
struct MessageIn {
    value: u64,
    annotation: String,
}

// ERR: can't find crate for `serde`, 'scale-info'
//#[gear_data]
struct MessageOut {
    old_value: u64,
    new_value: u64,
}

meta! {
    title: "Example program with metadata",
    input: MessageIn,
    output: MessageOut,
    init_input: MessageIn,
    init_output: MessageOut
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let message_in =
        MessageIn::decode(&mut &msg::load()[..]).expect("Failed to decode incoming message");
    let old_value = CURRENT_VALUE;
    CURRENT_VALUE += message_in.value;
    ext::debug(&format!(
        "Increased with annotation: {}",
        message_in.annotation
    ));

    msg::reply(
        &MessageOut {
            old_value,
            new_value: CURRENT_VALUE,
        }
        .encode(),
        1000000,
        0,
    )
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let message_in =
        MessageIn::decode(&mut &msg::load()[..]).expect("Failed to decode incoming message");
    CURRENT_VALUE = message_in.value;

    msg::reply(
        &MessageOut {
            old_value: 0,
            new_value: CURRENT_VALUE,
        }
        .encode(),
        1000000,
        0,
    )
}
