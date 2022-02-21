#![no_std]

use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use futures::{join, select_biased};
use gstd::{debug, msg, prelude::*, ActorId};

static mut DEMO_ASYNC: ActorId = ActorId::new([0u8; 32]);
static mut DEMO_PING: ActorId = ActorId::new([0u8; 32]);

const GAS_LIMIT: u64 = 5_000_000_000;

#[no_mangle]
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let dests: Vec<&str> = input.split(',').collect();
    if dests.len() != 2 {
        panic!("Invalid input, should be three IDs separated by comma");
    }
    DEMO_ASYNC = ActorId::from_slice(
        &hex::decode(dests[0]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    )
    .expect("Unable to create ActorId");
    DEMO_PING = ActorId::from_slice(
        &hex::decode(dests[1]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    )
    .expect("Unable to create ActorId");
}

#[gstd::async_main]
async fn main() {
    let source = msg::source();
    let command = String::from_utf8(msg::load_bytes()).expect("Unable to decode string");

    match command.as_ref() {
        // Directly using stream from futures unordered to step through each future done
        "unordered" => {
            debug!("UNORDERED: Before any sending");

            let requests = vec![
                msg::send_bytes_and_wait_for_reply(
                    unsafe { DEMO_ASYNC },
                    "START",
                    GAS_LIMIT * 10,
                    0,
                ),
                msg::send_bytes_and_wait_for_reply(unsafe { DEMO_PING }, "PING", GAS_LIMIT, 0),
            ];

            let mut unordered: FuturesUnordered<_> = requests.into_iter().collect();

            debug!("Before any polls");

            let first = unordered.next().await;
            msg::send_bytes(
                source,
                first.expect("Can't fail").expect("Exit code is 0"),
                0,
                0,
            );
            debug!("First (from demo_ping) done");

            let second = unordered.next().await;
            msg::send_bytes(
                source,
                second.expect("Can't fail").expect("Exit code is 0"),
                0,
                0,
            );
            debug!("Second (from demo_async) done");

            msg::reply_bytes("DONE", 0, 0);
        }
        // using select! macro to wait for first future done
        "select" => {
            debug!("SELECT: Before any sending");

            select_biased! {
                res = msg::send_bytes_and_wait_for_reply(unsafe { DEMO_ASYNC }, "START", GAS_LIMIT * 10, 0) => {
                    debug!("Recieved msg from demo_async");
                    msg::send_bytes(source, res.expect("Exit code is 0"), 0, 0);
                },
                res = msg::send_bytes_and_wait_for_reply(unsafe { DEMO_PING }, "PING", GAS_LIMIT, 0) => {
                    debug!("Recieved msg from demo_ping");
                    msg::send_bytes(source, res.expect("Exit code is 0"), 0, 0);
                },
            };

            debug!("Finish after select");

            msg::reply_bytes("DONE", 0, 0);
        }
        // using join! macros to wait all features done
        "join" => {
            debug!("JOIN: Before any sending");

            let res = join!(
                msg::send_bytes_and_wait_for_reply(
                    unsafe { DEMO_ASYNC },
                    "START",
                    GAS_LIMIT * 10,
                    0
                ),
                msg::send_bytes_and_wait_for_reply(unsafe { DEMO_PING }, "PING", GAS_LIMIT, 0)
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

            msg::send_bytes(source, result, 0, 0);
            msg::reply_bytes("DONE", 0, 0);
        }
        _ => {
            panic!("Unknown option");
        }
    }
}
