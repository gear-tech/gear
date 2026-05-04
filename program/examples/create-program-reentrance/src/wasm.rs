// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use gstd::{ActorId, CodeId, debug, msg, prog};

static mut DST_PROGRAM_ID: ActorId = ActorId::new([0u8; 32]);

type HandleParams = ([u8; 32], u128);

#[unsafe(no_mangle)]
extern "C" fn init() {
    let program_id: [u8; 32] = msg::load().expect("internal error: invalid payload");
    debug!(
        "[CREATE_PROGRAM_REENTRANCE::init] DST_PROGRAM_ID: 0x{}",
        hex::encode(program_id)
    );

    unsafe { DST_PROGRAM_ID = program_id.into() };
}

#[gstd::async_main]
async fn main() {
    let (code_hash, amount): HandleParams = msg::load().expect("internal error: invalid payload");
    debug!(
        "[CREATE_PROGRAM_REENTRANCE::handle] code_hash: 0x{}, amount: {}",
        hex::encode(code_hash),
        amount
    );

    let code_id: CodeId = code_hash.into();
    let dest_program_id = unsafe { DST_PROGRAM_ID };

    // Call gr_create_program syscall
    let (_msg_id, program_id) =
        prog::create_program_bytes(code_id, b"", b"", 0_u128).expect("Error creating program");
    debug!(
        "[CREATE_PROGRAM_REENTRANCE::handle] Created program with ID: 0x{}",
        hex::encode(program_id)
    );

    // Send a message to the `ping` contract and wait for reply
    let reply = msg::send_bytes_for_reply(dest_program_id, b"PING", 0, 0)
        .expect("Error sending message")
        .await
        .expect("Received error reply");
    assert_eq!(reply, b"PONG");
    debug!(
        "[CREATE_PROGRAM_REENTRANCE::handle] Received reply from remote program: {:?}",
        reply
    );

    // Try to transfer the `amount` of tokens to the destination program's account
    gstd::msg::send_bytes(dest_program_id, b"NOT_PING", amount).expect("Failed to send message");

    msg::reply_bytes(b"Done", 0).expect("Failed to send reply");
}
