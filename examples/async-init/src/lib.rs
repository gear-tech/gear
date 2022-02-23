#![no_std]

/* The program demonstrates asyncronous execution and
 * how to use macros `gstd::async_init`/`gstd::async_main`.
 *
 * `Init` method gets three addresses, sends empty messages
 * to them and waits for at least two replies with any payload ("approvals").
 *
 * `Handle` processes only "PING" messages. When `handle` gets such message
 * it sends empty requests to the three addresses and waits for just one approval.
 * If an approval is obtained the method replies with "PONG".
 */

use codec::Decode;
use futures::future;
use gstd::{msg, prelude::*, ActorId};
use scale_info::TypeInfo;

static mut APPROVER_FIRST: ActorId = ActorId::new([0u8; 32]);
static mut APPROVER_SECOND: ActorId = ActorId::new([0u8; 32]);
static mut APPROVER_THIRD: ActorId = ActorId::new([0u8; 32]);

#[derive(Debug, Decode, TypeInfo)]
pub struct InputArgs {
    pub approver_first: ActorId,
    pub approver_second: ActorId,
    pub approver_third: ActorId,
}

gstd::metadata! {
    title: "demo async init",
    init:
        input: InputArgs,
}

#[gstd::async_init]
async fn init() {
    let args: InputArgs = msg::load().expect("Failed to decode `InputArgs`");

    APPROVER_FIRST = args.approver_first;
    APPROVER_SECOND = args.approver_second;
    APPROVER_THIRD = args.approver_third;

    let mut requests: Vec<_> = [APPROVER_FIRST, APPROVER_SECOND, APPROVER_THIRD]
        .iter()
        .map(|s| msg::send_bytes_and_wait_for_reply(*s, b"", 0))
        .collect();

    let mut threshold = 0usize;
    while !requests.is_empty() {
        let (.., remaining) = future::select_all(requests).await;
        threshold += 1;

        if threshold >= 2 {
            break;
        }

        requests = remaining;
    }
}

#[gstd::async_main]
async fn main() {
    let message = msg::load_bytes();
    if message != b"PING" {
        return;
    }

    let requests: Vec<_> = [
        unsafe { APPROVER_FIRST },
        unsafe { APPROVER_SECOND },
        unsafe { APPROVER_THIRD },
    ]
    .iter()
    .map(|s| msg::send_bytes_and_wait_for_reply(*s, b"", 0))
    .collect();

    let _ = future::select_all(requests).await;

    msg::reply(b"PONG", 0);
}
