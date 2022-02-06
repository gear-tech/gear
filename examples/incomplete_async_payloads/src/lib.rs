#![no_std]

use core::num::ParseIntError;
use gstd::{debug, msg, prelude::*, ActorId};

const GAS_LIMIT: u64 = 1_000_000_000;

static mut DEMO_PING: ActorId = ActorId::new([0u8; 32]);

#[gstd::async_main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    match msg.as_ref() {
        "err common" => {
            debug!("err common processing");

            let handle = msg::send_init();
            handle.push(b"STORED ");

            let _ = msg::send_bytes_and_wait_for_reply(DEMO_PING, b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");

            debug!("err common processing awaken");

            // Got panic here providing reply with exit code 1
            handle.push("COMMON");

            handle.commit(msg::source(), GAS_LIMIT, 0);

            msg::send(msg::source(), "I'll not be sent", GAS_LIMIT, 0);
        }
        "err reply" => {
            debug!("err reply processing");

            msg::reply_push(b"STORED ");

            let _ = msg::send_bytes_and_wait_for_reply(DEMO_PING, b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");

            debug!("err reply processing awaken");

            msg::reply_push(b"REPLY");
            // Got no panic, but contains stripped payload
            msg::reply_commit(GAS_LIMIT, 0);

            msg::send_bytes(msg::source(), "I'll be sent", GAS_LIMIT, 0);
        }
        "ok common" => {
            debug!("ok common processing");
            let handle = msg::send_init();
            handle.push(b"OK PING");
            handle.commit(msg::source(), GAS_LIMIT, 0);
        }
        "ok reply" => {
            debug!("ok reply processing");
            msg::reply_push(b"OK REPLY");
            msg::reply_commit(GAS_LIMIT, 0);
        }
        _ => {}
    }
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    DEMO_PING =
        ActorId::from_slice(&decode_hex(&input).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"))
            .expect("Unable to create ActorId");
}
