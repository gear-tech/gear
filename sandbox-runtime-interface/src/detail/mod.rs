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

use codec::{Decode, Encode};
use gear_sandbox_native::sandbox as sandbox_env;
use once_cell::unsync::Lazy;
use sp_wasm_interface::{
    util,
    wasmtime::{AsContext, AsContextMut, Func, Val},
    Caller, FunctionContext, HostPointer, Pointer, StoreData, Value, WordSize,
};

mod instantiate;
pub use instantiate::method as instantiate;

mod invoke;
pub use invoke::method as invoke;

mod memory_new;
pub use memory_new::method as memory_new;

mod memory_get;
pub use memory_get::method as memory_get;

mod memory_set;
pub use memory_set::method as memory_set;

mod memory_teardown;
pub use memory_teardown::method as memory_teardown;

mod instance_teardown;
pub use instance_teardown::method as instance_teardown;

mod get_global_val;
pub use get_global_val::method as get_global_val;

mod set_global_val;
pub use set_global_val::method as set_global_val;

mod memory_grow;
pub use memory_grow::method as memory_grow;

mod memory_size;
pub use memory_size::method as memory_size;

mod get_buff;
pub use get_buff::method as get_buff;

mod get_instance_ptr;
pub use get_instance_ptr::method as get_instance_ptr;

struct Store {
    store_data_key: u64,
    store: sandbox_env::Store<Func>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            store_data_key: 0,
            store: sandbox_env::Store::new(sandbox_env::SandboxBackend::TryWasmer),
        }
    }

    pub fn get(&mut self, store_data_key: u64) -> &mut sandbox_env::Store<Func> {
        if self.store_data_key != store_data_key {
            self.store_data_key = store_data_key;
            self.store.clear();
        }

        &mut self.store
    }
}

static mut SANDBOX_STORE: Lazy<Store> = Lazy::new(Store::new);

pub fn init() {
    use std::sync::Once;

    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let storage = unsafe { &mut SANDBOX_STORE };
        let _ = storage.get(0);
    });
}

struct SandboxContext<'a, 'b> {
    caller: &'a mut Caller<'b, StoreData>,
    dispatch_thunk: Func,
    /// Custom data to propagate it in supervisor export functions
    state: u32,
}

impl<'a, 'b> sandbox_env::SandboxContext for SandboxContext<'a, 'b> {
    fn invoke(
        &mut self,
        invoke_args_ptr: Pointer<u8>,
        invoke_args_len: WordSize,
        func_idx: sandbox_env::SupervisorFuncIndex,
    ) -> gear_sandbox_native::error::Result<i64> {
        let mut ret_vals = [Val::null()];
        let result = self.dispatch_thunk.call(
            &mut self.caller,
            &[
                Val::I32(u32::from(invoke_args_ptr) as i32),
                Val::I32(invoke_args_len as i32),
                Val::I32(self.state as i32),
                Val::I32(usize::from(func_idx) as i32),
            ],
            &mut ret_vals,
        );

        match result {
            Ok(()) => {
                if let Some(ret_val) = ret_vals[0].i64() {
                    Ok(ret_val)
                } else {
                    Err("Supervisor function returned unexpected result!".into())
                }
            }
            Err(err) => Err(err.to_string().into()),
        }
    }

    fn read_memory_into(
        &self,
        address: Pointer<u8>,
        dest: &mut [u8],
    ) -> sp_wasm_interface::Result<()> {
        util::read_memory_into(self.caller.as_context(), address, dest)
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> sp_wasm_interface::Result<()> {
        util::write_memory_from(self.caller.as_context_mut(), address, data)
    }

    fn allocate_memory(&mut self, size: WordSize) -> sp_wasm_interface::Result<Pointer<u8>> {
        util::allocate_memory(self.caller, size)
    }

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> sp_wasm_interface::Result<()> {
        util::deallocate_memory(self.caller, ptr)
    }
}

fn read_memory(
    ctx: impl AsContext<Data = StoreData>,
    address: Pointer<u8>,
    size: WordSize,
) -> sp_wasm_interface::Result<Vec<u8>> {
    let mut vec = vec![0; size as usize];
    util::read_memory_into(ctx, address, &mut vec)?;
    Ok(vec)
}

fn trace(func: &str, caller: &Caller<'_, StoreData>) {
    let data_ptr: *const _ = caller.data();
    let caller_ptr: *const _ = caller;
    let thread_id = std::thread::current().id();

    log::trace!(
        "{func}; data_ptr = {:#x?}, caller_ptr = {:#x?}, thread_id = {:?}",
        data_ptr as u64,
        caller_ptr as u64,
        thread_id,
    );
}
