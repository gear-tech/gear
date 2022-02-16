#![no_std]

use gstd::{msg, prelude::*};

static mut COUNTER: usize = 0;

#[gstd::async_main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    if &msg == "async" {
        COUNTER += 1;
        let _ = msg::send_bytes_and_wait_for_reply(2.into(), b"PING", 100_000_000, 0)
            .await
            .expect("Error in async message processing");
        msg::reply(COUNTER as i32, 100_000_000, 0);

        COUNTER = 0;
    };
}
