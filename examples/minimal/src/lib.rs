#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{msg, prelude::*};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    msg::reply(b"Hello world!", 0, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    loop {}
}
