#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{msg, prelude::*};
use gstd_async::msg as msg_async;

const PING_PROGRAM_ID: u64 = 2;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let message = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    if message == "START" {
        gstd_async::block_on(handle_async());
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    gstd_async::block_on(handle_async());
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

async fn handle_async() {
    msg::send(0.into(), b"LOG", u64::MAX, 0);
    let another_reply =
        msg_async::send_and_wait_for_reply(PING_PROGRAM_ID.into(), b"PING", u64::MAX, 0).await;
    let another_reply = String::from_utf8(another_reply).expect("Invalid reply: should be utf-8");
    if another_reply == "PONG" {
        msg::reply(b"PING", u64::MAX, 0);
    }
}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    loop {}
}
