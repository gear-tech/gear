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

use sp_wasm_interface::StoreData;
use wasmtime::{Caller, Linker};

use crate::host::{api::MemoryWrap, threads};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_publish_promise", publish_promise)?;

    Ok(())
}

fn publish_promise(caller: Caller<'_, StoreData>, promise_ptr_len: i64) {
    let memory = MemoryWrap(caller.data().memory());

    threads::with_params(|params| {
        if let Some(ref sender) = params.promise_sender {
            let promise = memory.decode_by_val(&caller, promise_ptr_len);

            match sender.send(promise) {
                Ok(()) => {
                    log::trace!(
                        "successfully send promise to outer service: promise_ptr_len={promise_ptr_len}"
                    );
                }
                Err(err) => {
                    log::trace!(
                        "`publish_promise`: failed to send promise to receiver because of error={err}"
                    );
                }
            }
        }
    });
}
