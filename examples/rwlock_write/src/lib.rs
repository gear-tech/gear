#![no_std]

use core::num::ParseIntError;
use gstd::{exec, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;

static mut PING_DEST: ProgramId = ProgramId([0u8; 32]);
static RWLOCK: gstd_async::rwlock::RwLock<u32> = gstd_async::rwlock::RwLock::new(0);

const GAS_LIMIT: u64 = 500_000_000;

#[no_mangle]
pub unsafe extern "C" fn init() {
    let dest = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    PING_DEST = ProgramId::from_slice(
        &decode_hex(dest.as_ref()).expect("INTIALIZATION FAILED: INVALID DEST PROGRAM ID"),
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
        let _val = RWLOCK.write().await;

        let reply = msg_async::send_and_wait_for_reply(unsafe { PING_DEST }, b"PING", GAS_LIMIT, 0)
            .await
            .expect("Error in async message processing");

        if reply == b"PONG" {
            msg::reply(b"SUCCESS", exec::gas_available() - GAS_LIMIT, 0);
        } else {
            msg::reply(b"FAIL", exec::gas_available() - GAS_LIMIT, 0);
        }
    }
}
