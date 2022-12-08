#![no_std]

use futures::{
    join, select_biased,
    stream::{FuturesUnordered, StreamExt},
};
use gstd::{debug, msg, prelude::*, ActorId};

static mut DEMO_ASYNC: ActorId = ActorId::new([0u8; 32]);
static mut DEMO_PING: ActorId = ActorId::new([0u8; 32]);

#[no_mangle]
extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    let dests: Vec<&str> = input.split(',').collect();
    if dests.len() != 2 {
        panic!("Invalid input, should be three IDs separated by comma");
    }
    unsafe {
        DEMO_ASYNC = ActorId::from_slice(
            &hex::decode(dests[0]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
        )
        .expect("Unable to create ActorId");
        DEMO_PING = ActorId::from_slice(
            &hex::decode(dests[1]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
        )
        .expect("Unable to create ActorId");
    }
}

#[gstd::async_main]
async fn main() {
    let source = msg::source();
    let command = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Unable to decode string");

    match command.as_ref() {
        // Directly using stream from futures unordered to step through each future done
        "unordered" => {
            debug!("UNORDERED: Before any sending");

            let requests = vec![
                msg::send_bytes_for_reply(unsafe { DEMO_ASYNC }, "START", 0).unwrap(),
                msg::send_bytes_for_reply(unsafe { DEMO_PING }, "PING", 0).unwrap(),
            ];

            let mut unordered: FuturesUnordered<_> = requests.into_iter().collect();

            debug!("Before any polls");

            let first = unordered.next().await;
            msg::send_bytes(
                source,
                first.expect("Can't fail").expect("Exit code is 0"),
                0,
            )
            .unwrap();
            debug!("First (from demo_ping) done");

            let second = unordered.next().await;
            msg::send_bytes(
                source,
                second.expect("Can't fail").expect("Exit code is 0"),
                0,
            )
            .unwrap();
            debug!("Second (from demo_async) done");

            msg::reply_bytes("DONE", 0).unwrap();
        }
        // using select! macro to wait for first future done
        "select" => {
            debug!("SELECT: Before any sending");

            select_biased! {
                res = msg::send_bytes_for_reply(unsafe { DEMO_ASYNC }, "START", 0).unwrap() => {
                    debug!("Recieved msg from demo_async");
                    msg::send_bytes(source, res.expect("Exit code is 0"), 0).unwrap();
                },
                res = msg::send_bytes_for_reply(unsafe { DEMO_PING }, "PING", 0).unwrap() => {
                    debug!("Recieved msg from demo_ping");
                    msg::send_bytes(source, res.expect("Exit code is 0"), 0).unwrap();
                },
            };

            debug!("Finish after select");

            msg::reply_bytes("DONE", 0).unwrap();
        }
        // using join! macros to wait all features done
        "join" => {
            debug!("JOIN: Before any sending");

            let res = join!(
                msg::send_bytes_for_reply(unsafe { DEMO_ASYNC }, "START", 0).unwrap(),
                msg::send_bytes_for_reply(unsafe { DEMO_PING }, "PING", 0).unwrap()
            );

            debug!("Finish after join");

            let mut result = String::new();

            result.push_str(
                &String::from_utf8(res.0.expect("Exit code is 0"))
                    .expect("Unable to decode string"),
            );
            result.push_str(
                &String::from_utf8(res.1.expect("Exit code is 0"))
                    .expect("Unable to decode string"),
            );

            msg::send_bytes(source, result, 0).unwrap();
            msg::reply_bytes("DONE", 0).unwrap();
        }
        _ => {
            panic!("Unknown option");
        }
    }
}
