#![no_std]

use gstd::{
    msg::{self, MessageHandle},
    prelude::*,
};

#[no_mangle]
extern "C" fn handle() {
    let new_msg = String::from_utf8(gstd::msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");

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
    for (vertex, degree) in degrees.iter().enumerate().take(nodes) {
        if *degree == 1 {
            leaves.push(vertex);
        }
    }

    let handle = MessageHandle::init().unwrap();

    for vertex in &code {
        leaves.sort();
        leaves.reverse();
        let leaf = leaves.pop().expect("An error occured during calculating");

        handle
            .push(format!("[{}, {}];", leaf + 1, vertex + 1))
            .unwrap();

        degrees[*vertex] -= 1;

        if degrees[*vertex] == 1 {
            leaves.push(*vertex);
        }
    }

    handle
        .push(format!("[{}, {}]", leaves[0] + 1, leaves[1] + 1))
        .unwrap();

    handle.commit(msg::source(), 0).unwrap();
}
