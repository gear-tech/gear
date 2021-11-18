#![no_std]

use core::time::Duration;
use gstd::{debug, exec, msg, prelude::*};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let payload = String::from_utf8(msg::load_bytes()).expect("Invalid message");

    let bt = Duration::from_millis(exec::block_timestamp());
    debug!("Timestamp: {:?}", bt);

    let bh = exec::block_height();
    msg::reply_bytes(format!("{}_{}", payload, bh), 10_000_000, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
