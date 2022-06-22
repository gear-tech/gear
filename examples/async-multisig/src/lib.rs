#![no_std]

use codec::{Decode, Encode};
use core::convert::TryFrom;
use futures::{future, FutureExt};
use gstd::{msg, prelude::*, ActorId};
use scale_info::TypeInfo;
use sp_core::{
    sr25519::{Pair as Sr25519Pair, Public, Signature},
    Pair,
};

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

gstd::metadata! {
    title: "demo async multisig",
    init:
        input: InputArgs,
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let args: InputArgs = msg::load().expect("Failed to decode `InputArgs`");

    DESTINATION = args.destination;

    args.signatories
        .into_iter()
        .filter(|s| !SIGNATORIES.contains(s))
        .for_each(|s| SIGNATORIES.push(s));

    THRESHOLD = usize::try_from(args.threshold)
        .map(|t| t.min(SIGNATORIES.len()).max(1))
        .unwrap_or(1);
}

#[gstd::async_main]
async fn main() {
    let message = msg::load_bytes();

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
                        Signature::try_from(response.signature.as_slice()).ok()
                    })
            })
            .map(|signature| {
                let pub_key = Public((<[u8; 32]>::from(unsafe { SIGNATORIES[i] })));

                Sr25519Pair::verify(&signature, &message, &pub_key).into()
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
