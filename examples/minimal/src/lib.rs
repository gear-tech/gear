#![no_std]

use gstd::msg;

#[no_mangle]
extern "C" fn handle() {
    msg::reply(b"Hello world!", 0).unwrap();
}
