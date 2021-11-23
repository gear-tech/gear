#![no_std]

use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
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
    let requests = vec![
        msg::send_bytes_and_wait_for_reply(DEMO_ASYNC, "START", GAS_LIMIT * 10, 0),
        msg::send_bytes_and_wait_for_reply(DEMO_PING, "PING", GAS_LIMIT, 0),
    ];

    let mut unordered: FuturesUnordered<_> = requests.into_iter().collect();

    debug!("Before any polls");
    msg::reply_bytes(
        unordered
            .next()
            .await
            .expect("Can't fail")
            .expect("Exit code should be 0!"),
        0,
        0,
    );
    debug!("First (from demo_ping) done");
    msg::reply_bytes(
        unordered
            .next()
            .await
            .expect("Can't fail")
            .expect("Exit code should be 0!"),
        0,
        0,
    );
    debug!("Second (from demo_async) done");
}
