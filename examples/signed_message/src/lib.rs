#![no_std]

use codec::Decode;
use gstd::{debug, msg};
use sp_core::{
    crypto::{AccountId32, UncheckedFrom},
    H256,
};

static mut SIGNATORY: Option<AccountId32> = None;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let source = msg::source();
    let h256 = H256::from_slice(source.as_slice());
    let account = AccountId32::unchecked_from(h256);
    debug!("{:?}", account);

    if account == SIGNATORY.clone().unwrap() {
        msg::reply_bytes("Accepted!", 10_000_000, 0);
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let msg = msg::load_bytes();
    debug!("{:?}", msg);
    let account = AccountId32::decode(&mut &msg[..]).expect("AccountId::decode failed");
    debug!("{:?}", account);

    SIGNATORY = Some(account);
}
