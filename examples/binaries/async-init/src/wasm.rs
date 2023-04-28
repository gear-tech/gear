/* The program demonstrates asynchronous execution and
 * how to use macros `gstd::async_init`/`gstd::async_main`.
 *
 * `Init` method gets three addresses, sends "PING" messages
 * to them and waits for at least two replies with any payload ("approvals").
 *
 * `Handle` processes only "PING" messages. When `handle` gets such message
 * it sends empty requests to the three addresses and waits for just one approval.
 * If an approval is obtained the method replies with "PONG".
 */

use crate::InputArgs;
use futures::future;
use gstd::{msg, prelude::*, ActorId};

// One of the addresses supposed to be non-program.
static mut ARGUMENTS: InputArgs = InputArgs {
    approver_first: ActorId::zero(),
    approver_second: ActorId::zero(),
    approver_third: ActorId::zero(),
};

static mut RESPONSES: u8 = 0;

#[gstd::async_init]
async fn init() {
    let arguments: InputArgs = msg::load().expect("Failed to load arguments");

    let mut requests = arguments
        .iter()
        .map(|&addr| msg::send_bytes_for_reply(addr, "PING", 0).expect("Failed to send message"))
        .collect::<Vec<_>>();

    unsafe {
        ARGUMENTS = arguments;
    }

    while !requests.is_empty() {
        let (.., remaining) = future::select_all(requests).await;
        unsafe {
            RESPONSES += 1;
        }

        if unsafe { RESPONSES } >= 2 {
            break;
        }

        requests = remaining;
    }
}

#[gstd::async_main]
async fn main() {
    let message = msg::load_bytes().expect("Failed to load bytes");

    assert_eq!(message, b"PING");

    let requests = unsafe { ARGUMENTS.iter() }
        .map(|&addr| msg::send_bytes_for_reply(addr, "PING", 0).expect("Failed to send message"))
        .collect::<Vec<_>>();

    let _ = future::select_all(requests).await;

    msg::reply(unsafe { RESPONSES }, 0).expect("Failed to send reply");
}
