#![no_std]

use gstd::{ext, msg, prelude::*};
use gstd_meta::{meta, TypeInfo};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[derive(TypeInfo)]
pub struct MessageIn {
    pub value: u64,
    pub annotation: Vec<u8>,
}

#[derive(TypeInfo)]
pub struct MessageOut {
    pub old_value: u64,
    pub new_value: u64,
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
    let new_msg = String::from_utf8(msg::load_bytes()).expect("Invalid: should be utf-8");

    if new_msg == "PING" {
        msg::reply(b"PONG", 10_000_000, 0);
    }

    MESSAGE_LOG.push(new_msg);

    ext::debug(&format!(
        "{:?} total message(s) stored: ",
        MESSAGE_LOG.len()
    ));

    for log in MESSAGE_LOG.iter() {
        ext::debug(log);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
