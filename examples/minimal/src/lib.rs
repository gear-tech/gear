#![no_std]

use gstd::msg;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    msg::reply(b"Hello world!", 0).unwrap();
}
