#![no_std]

use gstd::{msg, prelude::ToString, ActorId};

static mut HOST: ActorId = ActorId::new([0u8; 32]);

#[no_mangle]
pub unsafe extern "C" fn handle() {}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    msg::send_bytes(HOST, msg::exit_code().to_string(), 0).unwrap();
}
