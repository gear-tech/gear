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

use ethexe_common::{HashOf, injected::Promise};
use gprimitives::MessageId;
use sp_wasm_interface::StoreData;
use wasmtime::{Caller, Linker};

use crate::host::{api::MemoryWrap, threads};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_forward_promise_to_service", forward_promise)?;

    Ok(())
}

// TODO: it is a raw implementation, should be fixed
fn forward_promise(
    caller: Caller<'_, StoreData>,
    encoded_reply_ptr_len: i64,
    message_id_ptr_len: i64,
) {
    let memory = MemoryWrap(caller.data().memory());

    let reply = memory.decode_by_val(&caller, encoded_reply_ptr_len);
    let message_id: MessageId = memory.decode_by_val(&caller, message_id_ptr_len);

    threads::with_params(|params| {
        if let Some(ref sender) = params.promise_sender {
            log::error!("calling `forward_promise` reply={reply:?}");

            let tx_hash = unsafe { HashOf::new(message_id.into_bytes().into()) };
            let promise = Promise { tx_hash, reply };

            match sender.send(promise) {
                Ok(()) => {
                    // log::trace!(
                    //     "successfully send promise to outer service: reply_ptr_len={reply_ptr_len}, message_id_ptr_len={message_id_ptr_len}"
                    // );
                }
                Err(err) => {
                    log::trace!("failed to send promise to outer service: error={err}");
                }
            }
        }
    });
}
