#![no_std]

use gstd::{exec, msg, prelude::*};

static mut MESSAGE: Option<String> = None;

fn value() -> String {
    unsafe { MESSAGE.clone().unwrap_or_else(|| String::from("EMPTY")) }
}

#[no_mangle]
extern "C" fn handle() {
    if let Ok(message) = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
    {
        // prev value
        msg::send_bytes(msg::source(), value(), 0).unwrap();

        unsafe { MESSAGE = Some(message.clone()) };

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
