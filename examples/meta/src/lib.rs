#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{ext, msg, prelude::*};
use gstd_meta::{meta, TypeInfo};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[allow(unused)]
#[derive(TypeInfo)]
struct MessageIn {
    value: u64,
    annotation: String,
}

#[allow(unused)]
#[derive(TypeInfo)]
struct MessageOut {
    old_value: u64,
    new_value: u64,
}

meta! {
    title: "Example program with metadata",
    input: MessageIn,
    output: MessageOut,
    init_input: MessageIn,
    init_output: MessageOut
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load_bytes()).expect("Invalid: should be utf-8");

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
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable();
}
