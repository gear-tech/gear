#![no_std]

use gcore::{ext, msg};
use gstd::prelude::*;

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg =
        String::from_utf8(gstd::msg::load_bytes()).expect("Invalid message: should be utf-8");

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
pub unsafe extern "C" fn handle_reply() {
    msg::reply(b"PONG", 10_000_000, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
