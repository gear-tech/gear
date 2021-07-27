#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::prelude::*;
use gstd::msg;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let m = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    if m == "call" {
        gstd_async::block_on(handle_async());
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

async fn handle_async() {
    msg::send(0.into(), b"async_result", u64::MAX, 0);
}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    loop {}
}
