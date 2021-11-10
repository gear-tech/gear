#![no_std]

use codec::Encode;
use core::str;
use gstd::{debug, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;
use scale_info::TypeInfo;

static mut SIGNATORY: ProgramId = ProgramId([0u8; 32]);
static mut SIGNED_MESSAGE_PROGRAM: ProgramId = ProgramId([0u8; 32]);

const GAS_LIMIT: u64 = 50_000_000_000;
const FINISH_STRING: &str = "FINISH: ";

#[derive(Debug, Encode, TypeInfo)]
struct SignRequest {
    message: Vec<u8>,
}

gstd::metadata! {
    title: "demo async sign",
    init:
        input: Vec<u8>,
    handle:
        input: Vec<u8>,
}

fn hex_to_id(hex: &str) -> Result<ProgramId, ()> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);

    hex::decode(hex)
        .map(|bytes| ProgramId::from_slice(&bytes))
        .map_err(|_| ())
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let dests: Vec<&str> = input.split(',').collect();
    if dests.len() != 2 {
        panic!("Invalid input, should be two IDs separated by comma");
    }
    SIGNATORY = hex_to_id(dests[0]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID");
    SIGNED_MESSAGE_PROGRAM = hex_to_id(dests[1]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID");
}

#[gstd_async::main]
async fn main() {
    let message = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    debug!("message = {:?}", message);
    if message == "START" {
        let encoded = SignRequest {
            message: b"PING".to_vec(),
        }
        .encode();

        let sign_response =
            msg_async::send_and_wait_for_reply(unsafe { SIGNATORY }, &encoded, GAS_LIMIT, 0).await;
        debug!("sign_response = {:?}", sign_response);

        let reply = msg_async::send_and_wait_for_reply(
            unsafe { SIGNED_MESSAGE_PROGRAM },
            &sign_response,
            GAS_LIMIT,
            0,
        )
        .await;
        debug!("reply = {:?}", reply);

        let result = format!(
            "{}{}",
            FINISH_STRING,
            str::from_utf8(&reply).expect("Failed to interpret `reply` as utf-8")
        );

        msg::reply_bytes(result, GAS_LIMIT, 0);
    }
}
