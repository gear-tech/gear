#![no_std]
#![allow(deprecated)]

use codec::{Decode, Encode};
use core::convert::TryFrom;
use futures::{future, FutureExt};
use gstd::{msg, prelude::*, ActorId};
use scale_info::TypeInfo;

static mut SIGNATORIES: Vec<ActorId> = vec![];
static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);
static mut THRESHOLD: usize = 0;

#[derive(Debug, Encode, TypeInfo)]
struct SignRequest {
    message: Vec<u8>,
}

#[derive(Debug, Decode, TypeInfo)]
pub struct SignResponse {
    pub message: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Decode, TypeInfo)]
pub struct InputArgs {
    pub threshold: u32,
    pub destination: ActorId,
    pub signatories: Vec<ActorId>,
}

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
gstd::metadata! {
    title: "demo async multisig",
    init:
        input: InputArgs,
}

#[no_mangle]
extern "C" fn init() {
    let args: InputArgs = msg::load().expect("Failed to decode `InputArgs`");

    unsafe {
        DESTINATION = args.destination;

        args.signatories
            .into_iter()
            .filter(|s| !SIGNATORIES.contains(s))
            .for_each(|s| SIGNATORIES.push(s));

        THRESHOLD = usize::try_from(args.threshold)
            .map(|t| t.clamp(1, SIGNATORIES.len()))
            .unwrap_or(1);
    }
}

#[gstd::async_main]
async fn main() {
    let message = msg::load_bytes().expect("Failed to load payload bytes");

    let encoded = SignRequest {
        message: message.clone(),
    }
    .encode();

    let mut requests: Vec<_> = unsafe { &SIGNATORIES }
        .iter()
        .enumerate()
        .map(|(i, s)| {
            msg::send_bytes_for_reply(*s, &encoded, 0).map(|fut| fut.map(move |r| (i, r)))
        })
        .collect::<Result<_, _>>()
        .unwrap();

    let mut threshold = 0usize;
    while !requests.is_empty() {
        let ((i, result), _, remaining) = future::select_all(requests).await;

        threshold += result
            .ok()
            .and_then(|bytes| {
                SignResponse::decode(&mut &bytes[..])
                    .ok()
                    .and_then(|response| {
                        // the same way as in verify.rs from subkey
                        <[u8; 64]>::try_from(response.signature.as_slice()).ok()
                    })
            })
            .map(|signature| {
                let pub_key = <[u8; 32]>::from(unsafe { SIGNATORIES[i] });

                light_sr25519::verify(&signature, &message, pub_key)
                    .is_ok()
                    .into()
            })
            .unwrap_or(0);

        if unsafe { THRESHOLD } <= threshold {
            msg::send_bytes(unsafe { DESTINATION }, message, 0).unwrap();
            break;
        } else if threshold + remaining.len() < unsafe { THRESHOLD } {
            // threshold can't be reached even if all remaining
            // programs correctly sign the message
            break;
        }

        requests = remaining;
    }
}
