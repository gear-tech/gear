#![no_std]

extern crate alloc;
extern crate galloc;

use alloc::vec::Vec;

#[no_mangle]
pub extern "C" fn memop() -> i64 {
    let mut arr = Vec::new();

    arr.push(42);
    // for i in 0..42 {
    //     arr.push(i);
    // }

    arr.len() as i64
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
