#![no_std]

use gstd::{debug, msg, prelude::*};

#[no_mangle]
extern "C" fn handle() {
    debug!("Hello from ping handle");

    let new_msg = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message");

    match new_msg.as_str() {
        "PING_REPLY_WITH_GAS" => {
            msg::reply_with_gas(b"pong reply with gas message", 3001, 0).unwrap();
        }
        "PING_REPLY_COMMIT_WITH_GAS" => {
            msg::reply_push(b"pong Part 1 ").unwrap();
            msg::reply_push(b"pong Part 2").unwrap();
            msg::reply_commit_with_gas(3001, 0).unwrap();
        }
        _ => {}
    }
}

#[no_mangle]
extern "C" fn init() {
    debug!("Hello from ping init");
}
