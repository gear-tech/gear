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
use ethexe_runtime_common::pack_u32_to_i64;
use gear_core::rpc::ReplyInfo;
use gprimitives::MessageId;
use parity_scale_codec::Encode;

interface::declare!(
    pub(super) fn ext_forward_promise_to_service(
        encoded_reply_ptr_len: i64,
        message_id_ptr_len: i64,
    );
);
pub fn send_promise(reply: &ReplyInfo, message_id: &MessageId) {
    unsafe {
        let message_id_ptr_len = pack_u32_to_i64(
            message_id.as_ref().as_ptr() as _,
            message_id.encoded_size() as _,
        );
        let encoded_reply = reply.encode();
        let encoded_reply_ptr_len =
            pack_u32_to_i64(encoded_reply.as_ptr() as _, reply.encoded_size() as _);

        sys::ext_forward_promise_to_service(encoded_reply_ptr_len, message_id_ptr_len);
    }
}
