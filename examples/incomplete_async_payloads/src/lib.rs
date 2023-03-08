#![no_std]

use core::num::ParseIntError;
use gstd::{
    debug,
    msg::{self, MessageHandle},
    prelude::*,
    ActorId,
};

static mut DEMO_PING: ActorId = ActorId::zero();

#[gstd::async_main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");

    match msg.as_ref() {
        "handle store" => {
            debug!("stored common processing");

            let handle = MessageHandle::init().unwrap();
            handle.push(b"STORED ").unwrap();

            let _ = msg::send_bytes_for_reply(unsafe { DEMO_PING }, b"PING", 0)
                .unwrap()
                .await
                .expect("Error in async message processing");

            debug!("stored common processing awaken");

            handle.push("COMMON").unwrap();

            handle.commit(msg::source(), 0).unwrap();
        }
        "reply store" => {
            debug!("stored reply processing");

            msg::reply_push(b"STORED ").unwrap();

            let _ = msg::send_bytes_for_reply(unsafe { DEMO_PING }, b"PING", 0)
                .unwrap()
                .await
                .expect("Error in async message processing");

            debug!("stored reply processing awaken");

            msg::reply_push(b"REPLY").unwrap();

            msg::reply_commit(0).unwrap();
        }
        "handle" => {
            debug!("ok common processing");
            let handle = MessageHandle::init().unwrap();
            handle.push(b"OK PING").unwrap();
            handle.commit(msg::source(), 0).unwrap();
        }
        "reply" => {
            debug!("ok reply processing");
            msg::reply_push(b"OK REPLY").unwrap();
            msg::reply_commit(0).unwrap();
        }
        "reply twice" => {
            debug!("reply twice processing");

            msg::reply_bytes("FIRST", 0).unwrap();

            let _ = msg::send_bytes_for_reply(unsafe { DEMO_PING }, b"PING", 0)
                .unwrap()
                .await
                .expect("Error in async message processing");

            debug!("reply twice processing awaken");

            // Won't be sent, because one
            // execution allows only one reply
            msg::reply_bytes("SECOND", 0).unwrap();
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
extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    unsafe {
        DEMO_PING = ActorId::from_slice(
            &decode_hex(&input).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
        )
        .expect("Unable to create ActorId")
    };
}
