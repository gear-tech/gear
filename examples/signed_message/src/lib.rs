#![no_std]

use codec::Decode;
use gstd::{debug, msg, prelude::*, ProgramId};
use sp_core::{
    crypto::{AccountId32, UncheckedFrom},
    sr25519,
};

use scale_info::TypeInfo;

#[derive(Debug, Decode, TypeInfo)]
struct InitArgs {
    account: AccountId32,
}

#[derive(Debug, Decode, TypeInfo)]
struct HandleArgs {
    message: Vec<u8>,
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

static mut SIGNATORY: Option<ProgramId> = None;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let args: HandleArgs = msg::load().expect("HandleArgs decode failed");
    debug!("{:?}", args);

    let mut signature = <sr25519::Pair as sp_core::Pair>::Signature::default();
    if args.signature.len() != AsRef::<[u8]>::as_ref(&signature).len() {
        debug!("wrong signature length");
        return;
    }

    signature.as_mut().copy_from_slice(&args.signature);

    let pub_key = <sr25519::Pair as sp_core::Pair>::Public::unchecked_from(
        SIGNATORY.expect("has to be inited").0,
    );
    debug!("{:?}", pub_key);
    if <sr25519::Pair as sp_core::Pair>::verify(&signature, &args.message, &pub_key) {
        // msg::send(SIGNATORY.unwrap(), b"Authorized PONG", 10_000_000, 0);
        let reply: &[u8] = if &args.message == b"PING" {
            b"Authorized PONG"
        } else {
            b"Authorized reply"
        };

        msg::reply_bytes(reply, 10_000_000, 0);
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let args: InitArgs = msg::load().expect("InitArgs decode failed");
    debug!("{:?}", args);

    let bytes: [u8; 32] = args.account.into();
    SIGNATORY = Some(ProgramId(bytes));
}
