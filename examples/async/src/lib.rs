#![no_std]
#![feature(default_alloc_error_handler)]

use core::num::ParseIntError;
use gstd::{msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;

static mut PING_PROGRAM_ID: ProgramId = ProgramId([0u8; 32]);

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
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");
    let send_to = ProgramId::from_slice(
        &decode_hex(&input).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    );
    PING_PROGRAM_ID = send_to;
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

async fn handle_async() {
    msg::send(0.into(), b"LOG", 0);
    let dest = unsafe { PING_PROGRAM_ID };
    let another_reply = msg_async::send_and_wait_for_reply(dest, b"PING", 50_000_000, 0).await;
    let another_reply = String::from_utf8(another_reply).expect("Invalid reply: should be utf-8");
    if another_reply == "PONG" {
        msg::reply(b"PING", 2_000_000, 0);
    }
}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    unsafe {
        core::arch::wasm32::unreachable();
    }
}
