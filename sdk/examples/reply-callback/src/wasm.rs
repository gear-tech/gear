// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
