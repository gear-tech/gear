// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{msg, prelude::*};

#[unsafe(no_mangle)]
extern "C" fn init() {}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let payload = msg::load_bytes().expect("Failed to load payload");
    let is_panic = payload[0] == 0x01;

    if is_panic {
        panic!();
    } else {
        let message_id = msg::id().into_bytes();

        // cast calldata "function replyOn_methodName(bytes32 messageId) external" "0x..."
        let mut payload = [0u8; 36];
        payload[..4].copy_from_slice(&[0xb5, 0x2a, 0xb5, 0x55]); // DemoCaller.replyOn_methodName.selector
        payload[4..].copy_from_slice(&message_id);

        msg::reply_bytes(payload, 0).expect("Failed to send reply");
    }
}
