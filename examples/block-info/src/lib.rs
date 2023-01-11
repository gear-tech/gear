#![no_std]

use core::time::Duration;
use gstd::{debug, exec, msg, prelude::*};

#[no_mangle]
extern "C" fn handle() {
    let payload = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message");

    let bt = Duration::from_millis(exec::block_timestamp());
    debug!("Timestamp: {:?}", bt);

    let bh = exec::block_height();
    msg::reply_bytes(format!("{payload}_{bh}"), 0).unwrap();
}

#[no_mangle]
extern "C" fn meta_state() -> *mut [i32; 2] {
    let timestamp = exec::block_timestamp();
    gstd::util::to_leak_ptr(timestamp.to_le_bytes())
}
