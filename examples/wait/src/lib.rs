#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{msg, prelude::*};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    msg::wait();
    // Unreachable code
    msg::send(msg::source(), b"UNREACHABLE", 1000_000);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    unsafe {
        core::arch::wasm32::unreachable();
    }
}
