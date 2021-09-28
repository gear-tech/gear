#![no_std]

use gstd::{msg, prelude::*};
use gstd_async::msg as msg_async;

static mut COUNTER: usize = 0;

fn increase() {
    unsafe {
        COUNTER += 1;
    }
}

fn get() -> usize {
    unsafe { COUNTER.clone() }
}

fn clear() {
    unsafe {
        COUNTER = 0;
    }
}

#[gstd_async::main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    if &msg == "async" {
        increase();
        msg_async::send_and_wait_for_reply(2.into(), b"PING", 10_000_000, 0).await;
        msg::reply(get() as i32, 10_000_000, 0);
        clear();
    };
}
