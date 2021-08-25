#![no_std]
#![feature(default_alloc_error_handler)]

use gcore::{msg, prelude::*};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    if new_msg == "PING" {
        msg::reply(b"PO", 10_000_000, 0);
        msg::reply_push(b"NG");
    }

    if new_msg == "PING PING PING" {
        let handle = msg::send_init();
        msg::send_push(&handle, b"PONG1");
        msg::send_push(&handle, b"PONG2");
        msg::send_push(&handle, b"PONG3");
        msg::send_commit(handle, msg::source(), 10_000_000, 0);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe {
        core::arch::wasm32::unreachable();
    }
}
