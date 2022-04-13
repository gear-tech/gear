#![no_std]

use gstd::{debug, msg};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    debug!("Starting reply with gas test");
    msg::reply_with_gas(b"reply with gas message", 42, 0).unwrap();

    debud!("Starting reply commit with gas test");
    msg::reply_push(b"Part 1").unwrap();
    msg::reply_push(b"Part 2").unwrap();
    msg::reply_commit_with_gas(42, 0).unwrap();
}
