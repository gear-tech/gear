#![no_std]

use codec::Decode;
use gstd::{debug, exec, msg, prelude::*, ActorId};
use scale_info::TypeInfo;
use sp_core::{
    crypto::{AccountId32, UncheckedFrom},
    sr25519::{Pair as Sr25519Pair, Public, Signature},
    Pair,
};

#[derive(Debug, Decode, TypeInfo)]
struct InitArgs {
    account: AccountId32,
}

#[derive(Debug, Decode, TypeInfo)]
pub struct HandleArgs {
    pub message: Vec<u8>,
    pub signature: Vec<u8>,
}

gstd::metadata! {
    title: "SignedMessage",
    init:
        input: InitArgs,
    handle:
        input: HandleArgs,
}

static mut SIGNATORY: Option<ActorId> = None;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let signatory: [u8; 32] = match SIGNATORY {
        None => {
            msg::reply_bytes(b"Uninitialized", 10_000, 0);
            return;
        }
        Some(s) => s.into(),
    };

    let args: HandleArgs = match msg::load() {
        Err(_) => {
            msg::reply_bytes(b"Failed to decode `HandleArgs`", 10_000, 0);
            return;
        }
        Ok(a) => a,
    };
    debug!("{:?}", args);

    // the same way as in verify.rs from subkey
    let mut signature = Signature::default();
    if args.signature.len() != signature.0.len() {
        msg::reply_bytes(b"Wrong signature length", 10_000, 0);
        return;
    }

    signature.as_mut().copy_from_slice(&args.signature);

    let pub_key = Public::unchecked_from(signatory);
    debug!("{:?}", pub_key);

    let reply: &[u8] = if Sr25519Pair::verify(&signature, &args.message, &pub_key) {
        b"Reply to signed message"
    } else {
        b"Incorrect signature"
    };
    msg::reply_bytes(reply, exec::gas_available() - 100_000_000, 0);
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let maybe_args: Result<InitArgs, _> = msg::load();
    debug!("{:?}", maybe_args);

    SIGNATORY = maybe_args
        .map_err(|_| msg::reply_bytes(b"Failed to decode `InitArgs`", 10_000, 0))
        .ok()
        .map(|a| ActorId::new(a.account.into()));
}
