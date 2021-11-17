#![no_std]

use gstd::{debug, msg, prelude::*};

const GAS_LIMIT: u64 = 50_000_000;

#[gstd::async_main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    match msg.as_ref() {
        "err common" => {
            debug!("err common processing");
            let handle = msg::send_init();
            handle.push(b"ERR PING");
            let _ = msg::send_bytes_and_wait_for_reply(2.into(), b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");
            // Got panic here without message
            msg::reply("I'll not be sent", GAS_LIMIT, 0);
        }
        "err reply" => {
            debug!("err reply processing");
            msg::reply_push(b"ERR PING");
            let _ = msg::send_bytes_and_wait_for_reply(2.into(), b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");
            // Got panic here without message
            msg::reply("I'll not be sent", GAS_LIMIT, 0);
        }
        "ok common" => {
            debug!("ok common processing");
            let handle = msg::send_init();
            handle.push(b"OK PING");
            handle.commit(msg::source(), GAS_LIMIT, 0);
            let _ = msg::send_bytes_and_wait_for_reply(2.into(), b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");
        }
        "ok reply" => {
            debug!("ok reply processing");
            msg::reply_push(b"OK REPLY");
            msg::reply_commit(GAS_LIMIT, 0);
            let _ = msg::send_bytes_and_wait_for_reply(2.into(), b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");
        }
        _ => {}
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
