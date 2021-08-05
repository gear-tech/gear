#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{msg, prelude::*};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    let code: Vec<usize> = new_msg.split_whitespace().map(|v| v.parse::<usize>()).collect();

    let nodes = &code.len() + 2;

    let mut degrees = vec![1; nodes];
    for vertex in &code {
        if *vertex < 1 || *vertex > nodes {
            msg::send(
                msg::source(),
                b"Invalid code",
                u64::MAX
            );
            return;
        }
        degrees[*vertex] += 1;
    }

    let mut leaves = vec![];
    for vertex in 0..nodes {
        if degrees[vertex] == 1 {
            leaves.push(vertex);
        }
    }

    let handle = msg::init(
        msg::source(),
        b"Graph edges:",
        u64::MAX
    )

    for vertex in &code {
        leaves.sort_unstable();
        leaves.reverse();
        let leaf = leaves.pop().expect("An error occured during calculating");

        msg::push(
            handle,
            format!(" ({} , {}) ", leaf + 1, vertex + 1).as_bytes()
        );

        degrees[*vertex] -= 1;

        if degrees[*vertex] == 1 {
            leaves.push(*vertex);
        }
    }

    msg::commit(handle);
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    unsafe { core::arch::wasm32::unreachable(); }
}
