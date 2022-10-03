#![no_std]

use gcore::msg;
use gstd::prelude::*;

static mut COUNTER: usize = 0;

#[no_mangle]
unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(gstd::msg::load_bytes().unwrap())
        .expect("Invalid message: should be utf-8");

    if new_msg == "PING" {
        msg::reply_push(b"PO").unwrap();
        msg::reply_push(b"NG").unwrap();
        msg::reply_commit(0).unwrap();
    }

    if new_msg == "PING PING PING" && COUNTER > 0 {
        let handle = msg::send_init().unwrap();
        msg::send_push(handle, b"PONG1").unwrap();
        msg::send_push(handle, b"PONG2").unwrap();
        msg::send_push(handle, b"PONG3").unwrap();
        msg::send_commit(handle, msg::source(), 0).unwrap();
    }

    COUNTER += 1;
}
