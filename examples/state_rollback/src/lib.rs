#![no_std]

use gstd::{exec, msg, prelude::*};

static mut MESSAGE: Option<String> = None;

fn value() -> String {
    unsafe { MESSAGE.clone().unwrap_or_else(|| String::from("EMPTY")) }
}

#[no_mangle]
unsafe extern "C" fn handle() {
    if let Ok(message) = String::from_utf8(msg::load_bytes().unwrap()) {
        // prev value
        msg::send_bytes(msg::source(), value(), 0).unwrap();

        MESSAGE = Some(message.clone());

        // new value
        msg::reply_bytes(value(), 0).unwrap();

        if message == "panic" {
            panic!();
        };

        if message == "leave" {
            exec::leave();
        }
    }
}
