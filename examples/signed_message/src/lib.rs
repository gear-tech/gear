#![no_std]

use codec::Decode;
use gstd::{debug, msg, prelude::*};
use sp_core::{
    crypto::{AccountId32, UncheckedFrom},
    H256,
    sr25519,
};

use scale_info::TypeInfo;

#[derive(Debug, Decode, TypeInfo)]
struct InitArgs {
    account: AccountId32,
}

#[derive(Debug, Decode, TypeInfo)]
struct HandleArgs {
    // now it just "PING"
    // message: Vec<u8>,
    signature: Vec<u8>,
}

gstd::metadata! {
    title: "SignedMessage",
        init:
            input: InitArgs,
            output: (),
        handle:
            input: HandleArgs,
            output: (),
}

static mut SIGNATORY: Option<<sr25519::Pair as sp_core::Pair>::Public> = None;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let args: HandleArgs = msg::load().expect("HandleArgs decode failed");
    debug!("{:?}", args);

    let mut signature = <sr25519::Pair as sp_core::Pair>::Signature::default();
    if args.signature.len() != AsRef::<[u8]>::as_ref(&signature).len() {
        debug!("if args.signature.len() != signature.as_ref().len() {");
        return;
    }

    signature.as_mut().copy_from_slice(&args.signature);
    if <sr25519::Pair as sp_core::Pair>::verify(&signature, &b"PING", &SIGNATORY.unwrap()) {
        msg::reply_bytes("Authorized PONG", 10_000_000, 0);
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let args: InitArgs = msg::load().expect("InitArgs decode failed");
    debug!("{:?}", args);

    let bytes: [u8; 32] = args.account.into();
    let pub_key = <sr25519::Pair as sp_core::Pair>::Public::unchecked_from(bytes);
    debug!("{:?}", pub_key);

    SIGNATORY = Some(pub_key);
}
