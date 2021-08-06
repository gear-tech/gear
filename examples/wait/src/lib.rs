#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{msg, prelude::*};

static mut STATE: u32 = 0;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    if STATE == 0 {
        STATE = 1;
        msg::wait();
    }
    // Unreachable code
    msg::send(msg::source(), b"WAITED", 1_000_000);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    unsafe {
        core::arch::wasm32::unreachable();
    }
}
