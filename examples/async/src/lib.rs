#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::prelude::*;
use gstd::ext;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    gstd_async::block_on(handle_async);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

async fn handle_async() {
    ext::debug("Async function call");
}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    loop {}
}
