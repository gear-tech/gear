#![no_std]

use gstd::{msg, ProgramId, prelude::ToString};

static mut HOST: ProgramId = ProgramId([0u8; 32]);

#[no_mangle]
pub unsafe extern "C" fn init() {
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    msg::send_bytes(ProgramId::from(3), "PING", 500_000_000_000, 0);
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    msg::send_bytes(HOST, msg::exit_code().to_string(), 500_000_000_000, 0);
}
