#![no_std]

use gcore::msg;
use gstd::prelude::*;

static mut COUNTER: usize = 0;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg =
        String::from_utf8(gstd::msg::load_bytes()).expect("Invalid message: should be utf-8");

    if new_msg == "PING" {
        msg::reply_push(b"PO");
        msg::reply_push(b"NG");
        msg::reply_commit(10_000_000, 0);
    }

    if new_msg == "PING PING PING" && COUNTER > 0 {
        let handle = msg::send_init();
        msg::send_push(&handle, b"PONG1");
        msg::send_push(&handle, b"PONG2");
        msg::send_push(&handle, b"PONG3");
        msg::send_commit(handle, msg::source(), 10_000_000, 0);
    }

    COUNTER += 1;
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
