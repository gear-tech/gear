#![no_std]

use core::num::ParseIntError;
use gstd::{debug, msg, msg::CodecMessageFuture, prelude::*, ActorId, CodeHash};

static mut DEST_0: ActorId = ActorId::new([0u8; 32]);
static mut DEST_1: ActorId = ActorId::new([0u8; 32]);
static mut DEST_2: ActorId = ActorId::new([0u8; 32]);

gstd::metadata! {
    title: "demo async",
    init:
        input: Vec<u8>,
        output: Vec<u8>,
    handle:
        input: Vec<u8>,
        output: Vec<u8>,
}

#[no_mangle]
unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let dests: Vec<&str> = input.split(',').collect();
    if dests.len() != 3 {
        panic!("Invalid input, should be three IDs separated by comma");
    }
    DEST_0 = ActorId::from_slice(
        &decode_hex(dests[0]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    )
    .expect("Unable to create ActorId");
    DEST_1 = ActorId::from_slice(
        &decode_hex(dests[1]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    )
    .expect("Unable to create ActorId");
    DEST_2 = ActorId::from_slice(
        &decode_hex(dests[2]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID"),
    )
    .expect("Unable to create ActorId");
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

#[gstd::async_main]
async fn main() {
    let message = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    match message.as_ref() {
        "START" => {
            let reply1 = msg::send_bytes_for_reply(unsafe { DEST_0 }, b"PING", 0)
                .expect("Error in sending message")
                .await
                .expect("Error in async message processing");
            let reply2 = msg::send_bytes_for_reply(unsafe { DEST_1 }, b"PING", 0)
                .expect("Error in sending message")
                .await
                .expect("Error in async message processing");
            let reply3 = msg::send_bytes_for_reply(unsafe { DEST_2 }, b"PING", 0)
                .expect("Error in sending message")
                .await
                .expect("Error in async message processing");

            if reply1 == b"PONG" && reply2 == b"PONG" && reply3 == b"PONG" {
                msg::reply(b"SUCCESS", 0).unwrap();
            } else {
                msg::reply(b"FAIL", 0).unwrap();
            }
        }
        "CREATE" => {
            let submitted_code: CodeHash = hex_literal::hex!(
                "504e575efaed45a1749531e69c871d1892af0368702392f7ae32a474e52fc4dd"
            )
            .into();

            let (first_program_id, first_init_future) = msg::create_program_wgas_wbytes_for_reply(
                submitted_code,
                b"unique",
                10_000_000_000,
                0,
            )
            .expect("Error in creating program");

            let init_answer = first_init_future
                .await
                .expect("Error in async init message processing");
            debug!("Got first init answer: {:?}", init_answer);

            let handle_reply = msg::send_bytes_for_reply(first_program_id, b"unique", 0)
                .expect("Error in sending message")
                .await
                .expect("Error in async message processing");
            debug!("Got first handle reply: {:?}", handle_reply);

            let (second_program_id, second_init_future): (ActorId, CodecMessageFuture<String>) =
                msg::create_program_wgas_for_reply(
                    submitted_code,
                    b"not unique",
                    10_000_000_000,
                    0,
                )
                .expect("Error in creating program");

            let init_answer = second_init_future
                .await
                .expect("Error in async init message processing");
            debug!("Got second init answer: {:?}", init_answer);

            let handle_reply = msg::send_bytes_for_reply(second_program_id, b"not unique", 0)
                .expect("Error in sending message")
                .await
                .expect("Error in async message processing");
            debug!("Got second handle reply: {:?}", handle_reply);
        }
        _ => debug!("Unknown command"),
    }
}
