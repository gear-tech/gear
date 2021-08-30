#![no_std]
#![feature(default_alloc_error_handler)]

use gcore::msg;
use gstd::prelude::*;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(gstd::msg::load_bytes()).expect("Invalid message: should be utf-8");

    let code: Vec<usize> = new_msg
        .split_whitespace()
        .map(|v| {
            v.parse::<usize>()
                .expect("Not a number was sent in sequence")
                - 1
        })
        .collect();

    let nodes = code.len() + 2;

    let mut degrees = vec![1; nodes];
    for vertex in &code {
        degrees[*vertex] += 1;
    }

    let mut leaves = vec![];
    for vertex in 0..nodes {
        if degrees[vertex] == 1 {
            leaves.push(vertex);
        }
    }

    let handle = msg::send_init();

    for vertex in &code {
        leaves.sort();
        leaves.reverse();
        let leaf = leaves.pop().expect("An error occured during calculating");

        msg::send_push(
            &handle,
            format!("[{}, {}];", leaf + 1, vertex + 1).as_bytes(),
        );

        degrees[*vertex] -= 1;

        if degrees[*vertex] == 1 {
            leaves.push(*vertex);
        }
    }

    msg::send_push(
        &handle,
        format!("[{}, {}]", leaves[0] + 1, leaves[1] + 1).as_bytes(),
    );

    msg::send_commit(handle, msg::source(), 10_000_000, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    unsafe {
        core::arch::wasm32::unreachable();
    }
}
