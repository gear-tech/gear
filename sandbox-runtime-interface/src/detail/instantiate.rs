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
    struct Context<'a> {
        store: &'a mut Store,
        result: u32,
        dispatch_thunk_id: u32,
        wasm_code: &'a [u8],
        raw_env_def: &'a [u8],
        state_ptr: Pointer<u8>,
    }

    let mut context = Context {
        store: unsafe { &mut SANDBOX_STORE },
        result: 0,
        dispatch_thunk_id,
        wasm_code,
        raw_env_def,
        state_ptr,
    };
    let context_ptr: *mut Context = &mut context;

    self_.with_caller_mut(context_ptr as *mut (), |context_ptr, caller| {
        let context_ptr: *mut Context = context_ptr.cast();
        let context: &mut Context =
            unsafe { context_ptr.as_mut().expect("instantiate; set above") };

        trace("instantiate", caller);

        // Extract a dispatch thunk from the instance's table by the specified index.
        let dispatch_thunk = {
            let table = caller
                .data()
                .table
                .expect("Runtime doesn't have a table; sandbox is unavailable");
            let table_item = table.get(caller.as_context_mut(), context.dispatch_thunk_id);

            *table_item
                .expect("dispatch_thunk_id is out of bounds")
                .funcref()
                .expect("dispatch_thunk_idx should be a funcref")
                .expect("dispatch_thunk_idx should point to actual func")
        };

        let data_ptr: *const _ = caller.data();
        let store_data_key = data_ptr as u64;
        let guest_env = match sandbox_env::GuestEnvironment::decode(
            context.store.get(store_data_key),
            context.raw_env_def,
        ) {
            Ok(guest_env) => guest_env,
            Err(_) => {
                context.result = sandbox_env::env::ERR_MODULE;
                return;
            }
        };

        // Catch any potential panics so that we can properly restore the sandbox store
        // which we've destructively borrowed.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            context.store.get(store_data_key).instantiate(
                context.wasm_code,
                guest_env,
                &mut SandboxContext {
                    caller,
                    dispatch_thunk,
                    state: context.state_ptr.into(),
                },
            )
        }));

        let result = match result {
            Ok(result) => result,
            Err(error) => std::panic::resume_unwind(error),
        };

        let instance_idx_or_err_code = match result {
            Ok(instance) => instance.register(context.store.get(store_data_key), dispatch_thunk),
            Err(sandbox_env::InstantiationError::StartTrapped) => sandbox_env::env::ERR_EXECUTION,
            Err(_) => sandbox_env::env::ERR_MODULE,
        };

        context.result = instance_idx_or_err_code
    });

    context.result
}
