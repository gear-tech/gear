// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{debug, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let size = msg::load::<i32>().expect("Failed to load `i32`") as usize;

    let request = format!("Request: size = {size}");

    debug!("{request}");
    unsafe { static_mut!(MESSAGE_LOG).push(request) };

    let vec = vec![42u8; size];
    let last_idx = size - 1;

    debug!("vec.len() = {:?}", vec.len());
    debug!(
        "vec[{last_idx}]: {:p} -> {:#04x}",
        &vec[last_idx], vec[last_idx]
    );

    msg::reply(size as i32, 0).expect("Failed to send reply");

    // The test idea is to allocate two wasm pages and check this allocation,
    // so we must skip `v` destruction.
    core::mem::forget(vec);

    let requests_amount = unsafe { static_ref!(MESSAGE_LOG).len() };
    debug!("Total requests amount: {requests_amount}");
    unsafe {
        static_ref!(MESSAGE_LOG)
            .iter()
            .for_each(|log| debug!("{log}"));
    }
}

// State-sharing function
#[unsafe(no_mangle)]
extern "C" fn state() {
    msg::reply(unsafe { static_ref!(MESSAGE_LOG).clone() }, 0).expect("Failed to share state");
}
