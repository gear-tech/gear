#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{ext, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg: i32 = msg::load().expect("Should be i32");
    MESSAGE_LOG.push(format!("New msg: {:?}", new_msg));
    let v = vec![1; new_msg as usize];
    ext::debug(&format!("v.len() = {:?}", v.len()));
    ext::debug(&format!(
        "v[{}]: {:p} -> {:#04x}",
        v.len() - 1,
        &v[new_msg as usize - 1],
        v[new_msg as usize - 1]
    ));
    msg::send(msg::source(), v.len() as i32, 10_000_000);
    ext::debug(&format!(
        "{:?} total message(s) stored: ",
        MESSAGE_LOG.len()
    ));

    for log in MESSAGE_LOG.iter() {
        ext::debug(log);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe {
        core::arch::wasm32::unreachable();
    }
}
