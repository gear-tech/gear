#![no_std]

use gstd::{msg, prelude::ToString, ProgramId};

static mut HOST: ProgramId = ProgramId([0u8; 32]);

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[no_mangle]
pub unsafe extern "C" fn handle() {}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    msg::send_bytes(HOST, msg::exit_code().to_string(), 0, 0);
}
