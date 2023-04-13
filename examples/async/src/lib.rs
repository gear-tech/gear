#![no_std]
#![allow(deprecated)]

use core::num::ParseIntError;
use gstd::{msg, prelude::*, ActorId};

static mut DEST_0: ActorId = ActorId::new([0u8; 32]);
static mut DEST_1: ActorId = ActorId::new([0u8; 32]);
static mut DEST_2: ActorId = ActorId::new([0u8; 32]);

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
gstd::metadata! {
    title: "demo async",
    init:
        input: Vec<u8>,
        output: Vec<u8>,
    handle:
        input: Vec<u8>,
        output: Vec<u8>,
}

#[no_mangle]
extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    let dests: Vec<&str> = input.split(',').collect();
    if dests.len() != 3 {
        panic!("Invalid input, should be three IDs separated by comma");
    }
    unsafe {
        DEST_0 = ActorId::from_slice(
            &decode_hex(dests[0]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
        )
        .expect("Unable to create ActorId");
        DEST_1 = ActorId::from_slice(
            &decode_hex(dests[1]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
        )
        .expect("Unable to create ActorId");
        DEST_2 = ActorId::from_slice(
            &decode_hex(dests[2]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
        )
        .expect("Unable to create ActorId");
    }
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
        let reply1 = msg::send_bytes_for_reply(unsafe { DEST_0 }, b"PING", 0)
            .expect("Error in sending message")
            .await
            .expect("Error in async message processing");
        let reply2 = msg::send_bytes_for_reply(unsafe { DEST_1 }, b"PING", 0)
            .expect("Error in sending message")
            .await
            .expect("Error in async message processing");
        let reply3 = msg::send_bytes_for_reply(unsafe { DEST_2 }, b"PING", 0)
            .expect("Error in sending message")
            .await
            .expect("Error in async message processing");

        if reply1 == b"PONG" && reply2 == b"PONG" && reply3 == b"PONG" {
            msg::reply(b"SUCCESS", 0).unwrap();
        } else {
            msg::reply(b"FAIL", 0).unwrap();
        }
    }
}
