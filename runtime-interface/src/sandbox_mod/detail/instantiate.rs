// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

use super::*;

pub fn method(
    self_: &mut dyn FunctionContext,
    dispatch_thunk_id: u32,
    wasm_code: &[u8],
    raw_env_def: &[u8],
    state_ptr: Pointer<u8>,
) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(self_, |caller| {
        trace("instantiate", caller);

        // Extract a dispatch thunk from the instance's table by the specified index.
        let dispatch_thunk = {
            let table = caller
                .data()
                .table
                .expect("Runtime doesn't have a table; sandbox is unavailable");
            let table_item = table.get(caller.as_context_mut(), dispatch_thunk_id);

            *table_item
                .expect("dispatch_thunk_id is out of bounds")
                .funcref()
                .expect("dispatch_thunk_idx should be a funcref")
                .expect("dispatch_thunk_idx should point to actual func")
        };

        let data_ptr: *const _ = caller.data();
        let store_data_key = data_ptr as u64;
        let guest_env = SANDBOXES.with(|sandboxes| {
            let mut store_ref = sandboxes
                .borrow_mut();
            let store = store_ref
                .get(store_data_key);

            sandbox_env::GuestEnvironment::decode(store, raw_env_def)
        });
        let Ok(guest_env) = guest_env else {
            method_result = sandbox_env::env::ERR_MODULE;
            return;
        };

        // Catch any potential panics so that we can properly restore the sandbox store
        // which we've destructively borrowed.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            SANDBOXES.with(|sandboxes| {
                sandboxes
                    .borrow_mut()
                    .get(data_ptr as u64)
                    .instantiate(
                        wasm_code,
                        guest_env,
                        &mut SandboxContext {
                            caller,
                            dispatch_thunk,
                            state: state_ptr.into(),
                        },
                    )
            })
        }));

        let result = match result {
            Ok(result) => result,
            Err(error) => std::panic::resume_unwind(error),
        };

        let instance_idx_or_err_code = match result {
            Ok(instance) => SANDBOXES.with(|sandboxes| {
                let mut store_ref = sandboxes
                    .borrow_mut();
                let store = store_ref
                    .get(store_data_key);

                instance.register(store, dispatch_thunk)
            }),
            Err(sandbox_env::InstantiationError::StartTrapped) => sandbox_env::env::ERR_EXECUTION,
            Err(_) => sandbox_env::env::ERR_MODULE,
        };

        method_result = instance_idx_or_err_code;
    });

    method_result
}
