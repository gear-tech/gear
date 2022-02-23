#![no_std]

use core::num::ParseIntError;
use gstd::{debug, msg, prelude::*, ActorId};

const GAS_LIMIT: u64 = 1_000_000_000;

static mut DEMO_PING: ActorId = ActorId::new([0u8; 32]);

#[gstd::async_main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    match msg.as_ref() {
        "handle store" => {
            debug!("stored common processing");

            let handle = msg::send_init();
            handle.push(b"STORED ");

            let _ = msg::send_bytes_and_wait_for_reply(unsafe { DEMO_PING }, b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");

            debug!("stored common processing awaken");

            handle.push("COMMON");

            handle.commit(msg::source(), GAS_LIMIT, 0);
        }
        "reply store" => {
            debug!("stored reply processing");

            msg::reply_push(b"STORED ");

            let _ = msg::send_bytes_and_wait_for_reply(unsafe { DEMO_PING }, b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");

            debug!("stored reply processing awaken");

            msg::reply_push(b"REPLY");

            msg::reply_commit(GAS_LIMIT, 0);
        }
        "handle" => {
            debug!("ok common processing");
            let handle = msg::send_init();
            handle.push(b"OK PING");
            handle.commit(msg::source(), GAS_LIMIT, 0);
        }
        "reply" => {
            debug!("ok reply processing");
            msg::reply_push(b"OK REPLY");
            msg::reply_commit(GAS_LIMIT, 0);
        }
        "reply twice" => {
            debug!("reply twice processing");

            msg::reply_bytes("FIRST", GAS_LIMIT, 0);

            let _ = msg::send_bytes_and_wait_for_reply(unsafe { DEMO_PING }, b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");

            debug!("reply twice processing awaken");

            // Won't be sent, because one
            // execution allows only one reply
            msg::reply_bytes("SECOND", GAS_LIMIT, 0);
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
