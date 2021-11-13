#![no_std]

use codec::{Decode, Encode};
use core::str;
use futures::{future, FutureExt};
use gstd::{debug, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;
use scale_info::TypeInfo;
use sp_core::{
    crypto::UncheckedFrom,
    sr25519::{Pair as Sr25519Pair, Public, Signature},
    Pair,
};

static mut SIGNATORIES: Vec<ProgramId> = vec![];
static mut SIGNED_MESSAGE_PROGRAM: ProgramId = ProgramId([0u8; 32]);
static mut THRESHOLD: usize = 0;

const GAS_LIMIT: u64 = 1_000_000_000;

#[derive(Debug, Encode, TypeInfo)]
struct SignRequest {
    message: Vec<u8>,
}

#[derive(Debug, Decode, TypeInfo)]
pub struct SignResponse {
    pub message: Vec<u8>,
    pub signature: Vec<u8>,
}

gstd::metadata! {
    title: "demo async multisig",
    init:
        input: Vec<u8>,
    handle:
        input: Vec<u8>,
}

fn hex_to_id(hex: &str) -> Result<ProgramId, ()> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);

    hex::decode(hex)
        .map(|bytes| ProgramId::from_slice(&bytes))
        .map_err(|_| ())
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let dests: Vec<&str> = input.split(',').collect();
    let len = dests.len();
    if len < 3 {
        panic!("Invalid input, should be a number and at least two IDs separated by comma");
    }

    THRESHOLD = usize::from_str_radix(dests[0], 10)
        .map(|t| if t > len - 2 { len - 2 } else { t })
        .map(|t| if t < 1 { 1 } else { t })
        .expect("INTIALIZATION FAILED: INVALID THRESHOLD");

    SIGNED_MESSAGE_PROGRAM = hex_to_id(dests[1]).expect("INTIALIZATION FAILED: INVALID PROGRAM ID");

    SIGNATORIES = dests
        .into_iter()
        .skip(2)
        .map(|s| hex_to_id(s).expect("INTIALIZATION FAILED: INVALID ACCOUNT ID"))
        .collect();
}

#[gstd_async::main]
async fn main() {
    let message = msg::load_bytes();
    debug!("message = {:?}", message);

    let encoded = SignRequest {
        message: message.clone(),
    }
    .encode();

    let mut requests: Vec<_> = unsafe { &SIGNATORIES }
        .iter()
        .enumerate()
        .map(|(i, s)| {
            msg_async::send_and_wait_for_reply(*s, &encoded, GAS_LIMIT, 0).map(move |r| (i, r))
        })
        .collect();

    let mut threshold = 0usize;
    while !requests.is_empty() {
        let ((i, result), _, remaining) = future::select_all(requests).await;

        threshold += result
            .ok()
            .map(|bytes| {
                SignResponse::decode(&mut &bytes[..])
                    .map(|response| {
                        // the same way as in verify.rs from subkey
                        let mut signature: Signature = Default::default();
                        if response.signature.len() == signature.0.len() {
                            signature.as_mut().copy_from_slice(&response.signature);
                            Some(signature)
                        } else {
                            None
                        }
                    })
                    .ok()
                    .flatten()
            })
            .flatten()
            .map(|signature| {
                let pub_key = Public::unchecked_from(unsafe { SIGNATORIES[i].0 });

                Sr25519Pair::verify(&signature, &message, &pub_key).into()
            })
            .unwrap_or(0);

        if unsafe { THRESHOLD } <= threshold {
            msg::send_bytes(unsafe { SIGNED_MESSAGE_PROGRAM }, message, GAS_LIMIT, 0);
            break;
        } else if threshold + remaining.len() < unsafe { THRESHOLD } {
            // threshold can't be reached even if all remaining
            // programs correctly sign the message
            break;
        }

        requests = remaining;
    }
}
