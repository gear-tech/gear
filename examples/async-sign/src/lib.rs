#![no_std]
#![allow(deprecated)]

use codec::{Decode, Encode};
use gstd::{msg, prelude::*, ActorId};
use scale_info::TypeInfo;

static mut SIGNATORY: ActorId = ActorId::new([0u8; 32]);
static mut DESTINATION: ActorId = ActorId::new([0u8; 32]);

#[derive(Debug, Encode, TypeInfo)]
struct SignRequest {
    message: Vec<u8>,
}

#[derive(Debug, Decode, TypeInfo)]
struct SignResponse {
    signature: Vec<u8>,
}

#[derive(Debug, Decode, TypeInfo)]
pub struct InputArgs {
    pub destination: ActorId,
    pub signatory: ActorId,
}

// NOTE: this macro has been deprecated, see
// https://github.com/gear-tech/gear/tree/master/examples/binaries/new-meta
gstd::metadata! {
    title: "demo async sign",
    init:
        input: InputArgs,
}

#[no_mangle]
extern "C" fn init() {
    let args: InputArgs = msg::load().expect("Failed to decode `InputArgs`");

    unsafe {
        DESTINATION = args.destination;
        SIGNATORY = args.signatory;
    }
}

#[gstd::async_main]
async fn main() {
    let message = msg::load_bytes().expect("Failed to load payload bytes");

    let request = SignRequest { message };

    let sign_response: Result<SignResponse, _> =
        msg::send_for_reply_as(unsafe { SIGNATORY }, &request, 0)
            .unwrap()
            .await;

    let verified = sign_response
        .ok()
        .and_then(|r| {
            // the same way as in verify.rs from subkey
            let mut signature = [0u8; 64];
            if r.signature.len() == signature.len() {
                signature.as_mut().copy_from_slice(&r.signature);
                Some(signature)
            } else {
                None
            }
        })
        .map(|signature| {
            let pub_key = <[u8; 32]>::from(unsafe { SIGNATORY });

            light_sr25519::verify(&signature, &request.message, pub_key).is_ok()
        })
        .unwrap_or(false);

    if verified {
        msg::send_bytes(unsafe { DESTINATION }, request.message, 0).unwrap();
    }
}
