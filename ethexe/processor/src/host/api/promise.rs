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

use core::mem::size_of;
use ethexe_common::{HashOf, injected::Promise};
use gear_core::rpc::ReplyInfo;
use gprimitives::MessageId;
use parity_scale_codec::{Decode, Error as CodecError};
use sp_wasm_interface::StoreData;
use wasmtime::{Caller, Linker};

use crate::host::{api::MemoryWrap, threads};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_promise_send_to_service", send_promise);

    Ok(())
}

fn send_promise(
    caller: Caller<'_, StoreData>,
    reply_ptr: i32,
    encoded_reply_len: i32,
    message_id_ptr: i32,
) -> Result<(), CodecError> {
    let memory = MemoryWrap(caller.data().memory());

    let reply_slice = memory.slice_mut(&caller, reply_ptr as usize, encoded_reply_len as usize);
    let reply = ReplyInfo::decode(reply_slice)?;

    let message_id_slice =
        memory.slice_mut(&caller, message_id_ptr as usize, size_of::<[u8; 32]>());
    let message_id = MessageId::decode(message_id_slice)?;

    threads::with_params(|params| {
        let tx_hash = unsafe { HashOf::new(message_id.into_bytes().into()) };
        let promise = Promise { tx_hash, reply };
        params.promise_sender.send(promise);
    });
    Ok(())
}
