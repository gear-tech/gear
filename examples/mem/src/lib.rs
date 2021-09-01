#![no_std]
#![feature(default_alloc_error_handler)]

use gcore::msg;
use gstd::prelude::*;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let data = vec![0u8; 32768];
    msg::reply(&data, 0, 0);
    panic!()
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {}

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable();
}
