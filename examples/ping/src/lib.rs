#![no_std]
#![feature(alloc_error_handler)]

#[cfg(target_arch = "wasm32")]
extern crate galloc;

use core::str;
use galloc::prelude::vec;
use gcore::msg;

#[no_mangle]
unsafe extern "C" fn handle() {
    let mut bytes = vec![0; msg::size() as usize];
    msg::read(&mut bytes).unwrap();

    if let Ok(received_msg) = str::from_utf8(&bytes) {
        if received_msg == "PING" {
            let _ = msg::reply(b"PONG", 0);
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
    core::arch::wasm32::unreachable()
}

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable();
}
