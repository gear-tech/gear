#![no_std]

use core::num::ParseIntError;
use gstd::{exec, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;

static mut DEST_0: ProgramId = ProgramId([0u8; 32]);
static mut DEST_1: ProgramId = ProgramId([0u8; 32]);
static mut DEST_2: ProgramId = ProgramId([0u8; 32]);

const GAS_LIMIT: u64 = 5_000_000_000;

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
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let dests: Vec<&str> = input.split(',').collect();
    if dests.len() != 3 {
        panic!("Invalid input, should be three IDs separated by comma");
    }
    DEST_0 = ProgramId::from_slice(
        &decode_hex(dests[0]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    );
    DEST_1 = ProgramId::from_slice(
        &decode_hex(dests[1]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    );
    DEST_2 = ProgramId::from_slice(
        &decode_hex(dests[2]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    );
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

#[gstd_async::main]
async fn main() {
    let message = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    if message == "START" {
        let reply1 =
            msg_async::send_and_wait_for_reply(unsafe { DEST_0 }, b"PING", GAS_LIMIT, 0).await;
        let reply2 =
            msg_async::send_and_wait_for_reply(unsafe { DEST_1 }, b"PING", GAS_LIMIT, 0).await;
        let reply3 =
            msg_async::send_and_wait_for_reply(unsafe { DEST_2 }, b"PING", GAS_LIMIT, 0).await;

        if reply1 == b"PONG" && reply2 == b"PONG" && reply3 == b"PONG" {
            msg::reply(b"SUCCESS", exec::gas_available() - GAS_LIMIT, 0);
        } else {
            msg::reply(b"FAIL", exec::gas_available() - GAS_LIMIT, 0);
        }
    }
}
