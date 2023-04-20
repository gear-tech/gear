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
    instance_idx: u32,
    function: &str,
    args: &[u8],
    return_val_ptr: Pointer<u8>,
    return_val_len: u32,
    state_ptr: Pointer<u8>,
) -> u32 {
    use sandbox_env::SandboxContext as _;

    struct Context<'a> {
        store: &'a mut Store,
        result: u32,
        instance_idx: u32,
        function: &'a str,
        args: &'a [u8],
        return_val_ptr: Pointer<u8>,
        return_val_len: u32,
        state_ptr: Pointer<u8>,
    }

    let mut context = Context {
        store: unsafe { &mut SANDBOX_STORE },
        result: u32::MAX,
        instance_idx,
        function,
        args,
        return_val_ptr,
        return_val_len,
        state_ptr,
    };
    let context_ptr: *mut Context = &mut context;

    self_.with_caller_mut(context_ptr as *mut (), |context_ptr, caller| {
        let context_ptr: *mut Context = context_ptr.cast();
        let context: &mut Context = unsafe { context_ptr.as_mut().expect("invoke; set above") };

        trace("invoke", caller);
        log::trace!("invoke, instance_idx={}", context.instance_idx);

        // Deserialize arguments and convert them into wasmi types.
        let args = Vec::<sp_wasm_interface::Value>::decode(&mut context.args)
            .expect("Can't decode serialized arguments for the invocation")
            .into_iter()
            .collect::<Vec<_>>();

        let data_ptr: *const _ = caller.data();
        let store_data_key = data_ptr as u64;
        let instance = context
            .store
            .get(store_data_key)
            .instance(context.instance_idx)
            .expect("backend instance not found");

        let dispatch_thunk = context
            .store
            .get(store_data_key)
            .dispatch_thunk(context.instance_idx)
            .expect("dispatch_thunk not found");

        let mut sandbox_context = SandboxContext {
            caller,
            dispatch_thunk,
            state: context.state_ptr.into(),
        };
        let result = instance.invoke(context.function, &args, &mut sandbox_context);

        context.result = match result {
            Ok(None) => sandbox_env::env::ERR_OK,
            Ok(Some(val)) => {
                // Serialize return value and write it back into the memory.
                sp_wasm_interface::ReturnValue::Value(val).using_encoded(|val| {
                    if val.len() > context.return_val_len as usize {
                        panic!("Return value buffer is too small");
                    }

                    sandbox_context
                        .write_memory(context.return_val_ptr, val)
                        .expect("can't write return value");

                    sandbox_env::env::ERR_OK
                })
            }
            Err(e) => {
                log::trace!("e = {e:?}");

                sandbox_env::env::ERR_EXECUTION
            }
        };
    });

    context.result
}
