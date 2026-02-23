// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::wasm::interface;
use gear_core::rpc::ReplyInfo;
use gprimitives::MessageId;
use parity_scale_codec::Encode;

interface::declare!(
    pub(super) fn send_promise(
        reply_ptr: *const ReplyInfo,
        encoded_reply_len: i32,
        message_id_ptr: *const MessageId,
    );
);

pub fn send_promise(reply: &ReplyInfo, message_id: &MessageId) {
    unsafe {
        // TODO: implement `as_ptr` for `ReplyInfo`
        let reply_ptr = 0;
        let reply_encoded_size = reply.encoded_size();
        let message_id_ptr = message_id.as_ref().as_ptr();

        sys::send_promise(
            reply_ptr as _,
            reply_encoded_size as i32,
            message_id_ptr as _,
        );
    }
}
