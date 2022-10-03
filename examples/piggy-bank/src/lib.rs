#![no_std]

use gstd::{debug, exec, msg};

#[no_mangle]
unsafe extern "C" fn handle() {
    let available_value = exec::value_available();
    debug!("inserted: {}, total: {}", msg::value(), available_value);

    if msg::load_bytes().unwrap() == b"smash" {
        debug!("smashing, total: {}", available_value);
        msg::reply_bytes(b"send", available_value).unwrap();
    }
}
