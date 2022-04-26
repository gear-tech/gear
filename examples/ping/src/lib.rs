#![no_std]

use gstd::{debug, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
pub unsafe extern "C" fn handle() {
    debug!("Hello from ping handle");

    let new_msg = String::from_utf8(msg::load_bytes()).expect("Invalid message");

    match new_msg.as_str() {
        "PING" => {
            msg::reply_bytes("PONG", 0).unwrap();
        }
        "PING_REPLY_WITH_GAS" => {
            msg::reply_with_gas(b"pong reply with gas message", 1, 0).unwrap();
        }
        "PING_REPLY_COMMIT_WITH_GAS" => {
            msg::reply_push(b"pong Part 1 ").unwrap();
            msg::reply_push(b"pong Part 2").unwrap();
            msg::reply_commit_with_gas(1, 0).unwrap();
        }
        _ => {}
    }

    MESSAGE_LOG.push(new_msg);

    debug!("{:?} total message(s) stored: ", MESSAGE_LOG.len());

    for log in MESSAGE_LOG.iter() {
        debug!(log);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    debug!("Hello from ping init");
}
