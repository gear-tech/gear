#![no_std]

use gstd::{debug, exec, msg};

#[no_mangle]
extern "C" fn handle() {
    debug!("Starting program id");
    let program_id = exec::program_id();
    debug!("My program id: {:?}", program_id);
    msg::reply(b"program_id", 0).unwrap();
}
