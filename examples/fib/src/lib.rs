#![no_std]
#![feature(default_alloc_error_handler)]

use core::convert::TryInto;
use gstd::{ext, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

fn make_fib(n: usize) -> Vec<i32> {
    let mut x = vec![1, 1];
    for i in 2..n {
        let next_x = x[i - 1] + x[i - 2];
        x.push(next_x)
    }
    x
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = i32::from_le_bytes(msg::load().try_into().expect("Should be i32"));
    MESSAGE_LOG.push(format!("New msg: {:?}", new_msg));

    msg::send(
        msg::source(),
        &make_fib(new_msg as usize)[new_msg as usize - 1].to_ne_bytes(),
        u64::MAX,
        0,
    );

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
fn panic(_info: &panic::PanicInfo) -> ! {
    unsafe {
        core::arch::wasm32::unreachable();
    }
}
