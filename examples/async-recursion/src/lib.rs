#![no_std]

use async_recursion::async_recursion;
use core::num::ParseIntError;
use gstd::{msg, prelude::*, ActorId};

static mut DEST: ActorId = ActorId::new([0u8; 32]);

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

#[no_mangle]
extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    unsafe {
        DEST = ActorId::from_slice(
            &decode_hex(&input).expect("Initialization failed: invalid program ID"),
        )
        .expect("Unable to create ActorId");
    }
}

/// Send message "PING" and wait for a reply, then recursively
/// repeat with `val` decreased by reply len while `val` > reply len.
#[async_recursion]
async fn rec_func(val: usize) {
    let reply = msg::send_bytes_for_reply(unsafe { DEST }, b"PING", 0)
        .expect("Error in sending message")
        .await
        .expect("Error in async message processing");

    msg::send_bytes(msg::source(), format!("Hello, val = {val}"), 0).unwrap();

    if val - reply.len() > 0 {
        rec_func(val - reply.len()).await;
    }
}

#[gstd::async_main]
async fn main() {
    rec_func(100).await;
}
