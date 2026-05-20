// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{ActorId, msg};

static mut BENEFICIARY: ActorId = ActorId::zero();

#[unsafe(no_mangle)]
extern "C" fn init() {
    // Original program creator becomes the beneficiary
    let dest = msg::source();
    unsafe { BENEFICIARY = dest };
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let (gas_limit, value): (u64, u128) = msg::load().expect("Failed to load handle payload");
    let value_receiver = unsafe { BENEFICIARY };
    msg::send_bytes_with_gas(value_receiver, [], gas_limit, value).unwrap();
}
