#![no_std]
#![feature(default_alloc_error_handler)]

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
    let new_msg: i32 = msg::load().expect("Should be i32");
    MESSAGE_LOG.push(format!("New msg: {:?}", new_msg));

    msg::send(
        msg::source(),
        make_fib(new_msg as usize)[new_msg as usize - 1],
        10_000_000,
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
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable();
}
