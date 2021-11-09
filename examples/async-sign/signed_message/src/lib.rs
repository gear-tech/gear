#![no_std]

use codec::Decode;
use common::{
    gstd::{self, debug, exec, msg, ProgramId},
    HandleArgs,
};
use scale_info::TypeInfo;
use sp_core::{
    crypto::{AccountId32, UncheckedFrom},
    sr25519,
};

#[derive(Debug, Decode, TypeInfo)]
struct InitArgs {
    account: AccountId32,
}

gstd::metadata! {
    title: "SignedMessage",
        init:
            input: InitArgs,
        handle:
            input: HandleArgs,
}

static mut SIGNATORY: Option<ProgramId> = None;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let signatory = match SIGNATORY {
        None => {
            msg::reply_bytes(b"Uninitialized", 10_000, 0);
            return;
        }
        Some(s) => s.0,
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
    let mut signature = <sr25519::Pair as sp_core::Pair>::Signature::default();
    if args.signature.len() != AsRef::<[u8]>::as_ref(&signature).len() {
        msg::reply_bytes(b"Wrong signature length", 10_000, 0);
        return;
    }

    signature.as_mut().copy_from_slice(&args.signature);

    let pub_key = <sr25519::Pair as sp_core::Pair>::Public::unchecked_from(signatory);
    debug!("{:?}", pub_key);

    let reply: &[u8] =
        if <sr25519::Pair as sp_core::Pair>::verify(&signature, &args.message, &pub_key) {
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

    let bytes: [u8; 32] = match maybe_args {
        Err(_) => {
            msg::reply_bytes(b"Failed to decode `InitArgs`", 10_000, 0);
            return;
        }
        Ok(args) => args.account.into(),
    };

    SIGNATORY = Some(ProgramId(bytes));
}
