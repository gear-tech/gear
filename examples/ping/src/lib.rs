#![no_std]

use gstd::{debug, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
pub unsafe extern "C" fn handle() {
    debug!("Hello from ping handle");

    let new_msg = String::from_utf8(msg::load_bytes()).expect("Invalid message");

    if new_msg == "PING" {
        msg::reply_bytes("PONG", 0).unwrap();
    }

    MESSAGE_LOG.push(new_msg);

    debug!("{:?} total message(s) stored: ", MESSAGE_LOG.len());

    for log in MESSAGE_LOG.iter() {
        debug!(log);
    }

    debug!("Starting reply with gas test");
    msg::reply_with_gas(b"reply with gas message", 42, 0).unwrap();

    debug!("Starting reply commit with gas test");
    msg::reply_push(b"Part 1").unwrap();
    msg::reply_push(b"Part 2").unwrap();
    msg::reply_commit_with_gas(42, 0).unwrap();
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    debug!("Hello from ping init");
}
