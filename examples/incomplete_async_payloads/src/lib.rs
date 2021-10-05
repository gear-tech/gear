#![no_std]

use gstd::{ext, msg, prelude::*};
use gstd_async::msg as msg_async;

const GAS_LIMIT: u64 = 5_000_000;

#[gstd_async::main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    match msg.as_ref() {
        "err common" => {
            ext::debug("err common processing");
            let handle = msg::send_init();
            handle.push(b"ERR PING");
            msg_async::send_and_wait_for_reply(2.into(), b"PING", GAS_LIMIT, 0).await;
            // Got panic here without message
            msg::reply("I'll not be sent", GAS_LIMIT, 0);
        }
        "err reply" => {
            ext::debug("err reply processing");
            msg::reply_push(b"ERR PING");
            msg_async::send_and_wait_for_reply(2.into(), b"PING", GAS_LIMIT, 0).await;
            // Got panic here without message
            msg::reply("I'll not be sent", GAS_LIMIT, 0);
        }
        "ok common" => {
            ext::debug("ok common processing");
            let handle = msg::send_init();
            handle.push(b"OK PING");
            handle.commit(msg::source(), GAS_LIMIT, 0);
            msg_async::send_and_wait_for_reply(2.into(), b"PING", GAS_LIMIT, 0).await;
        }
        "ok reply" => {
            ext::debug("ok reply processing");
            msg::reply_push(b"OK REPLY");
            msg::reply_commit(GAS_LIMIT, 0);
            msg_async::send_and_wait_for_reply(2.into(), b"PING", GAS_LIMIT, 0).await;
        }
        _ => {}
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
