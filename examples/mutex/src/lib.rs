#![no_std]

use core::num::ParseIntError;
use gstd::{lock::Mutex, msg, prelude::*, ActorId};

static mut PING_DEST: ActorId = ActorId::new([0u8; 32]);
static MUTEX: Mutex<u32> = Mutex::new(0);

#[no_mangle]
extern "C" fn init() {
    let dest = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    unsafe {
        PING_DEST = ActorId::from_slice(
            &decode_hex(dest.as_ref()).expect("INTIALIZATION FAILED: INVALID DEST PROGRAM ID"),
        )
        .expect("Unable to create ActorId")
    };
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

#[gstd::async_main]
async fn main() {
    let message = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    if message == "START" {
        let _val = MUTEX.lock().await;

        let reply = msg::send_bytes_for_reply(unsafe { PING_DEST }, b"PING", 0)
            .unwrap()
            .await
            .expect("Error in async message processing");

        if reply == b"PONG" {
            msg::reply(b"SUCCESS", 0).unwrap();
        } else {
            msg::reply(b"FAIL", 0).unwrap();
        }
    }
}
