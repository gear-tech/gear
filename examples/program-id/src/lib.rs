#![no_std]

// for panic/oom handlers
use gstd::{exec, msg};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    gstd::debug!("Starting program id");
    let program_id = exec::actor_id();
    gstd::debug!("My program id {:?}", program_id);
    msg::reply(b"program_id", 0, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
