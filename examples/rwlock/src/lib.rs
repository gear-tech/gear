#![no_std]

use gstd::{exec, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;

static mut PING_DEST: ProgramId = ProgramId([0u8; 32]);
static RWLOCK: gstd_async::rwlock::RwLock<u32> = gstd_async::rwlock::RwLock::new(0);

const GAS_LIMIT: u64 = 50_000_000;

#[no_mangle]
pub unsafe extern "C" fn init() {
    let dest = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    PING_DEST = ProgramId::from_slice(&hex::decode(dest).expect("Invalid hex"));
}

#[gstd_async::main]
async fn main() {
    let message = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    match message.as_ref() {
        "get" => {
            msg::reply(*RWLOCK.read().await, exec::gas_available() - GAS_LIMIT, 0);
        }
        "inc" => {
            let mut val = RWLOCK.write().await;
            *val += 1;
        }
        "ping&get" => {
            msg_async::send_and_wait_for_reply(unsafe { PING_DEST }, b"PING", GAS_LIMIT * 2, 0)
                .await;
            msg::reply(*RWLOCK.read().await, exec::gas_available() - GAS_LIMIT, 0);
        }
        "inc&ping" => {
            let mut val = RWLOCK.write().await;
            *val += 1;
            msg_async::send_and_wait_for_reply(
                unsafe { PING_DEST },
                b"PING",
                exec::gas_available() - GAS_LIMIT,
                0,
            )
            .await;
        }
        _ => {
            RWLOCK.write().await;
            RWLOCK.read().await;
        }
    }
}
