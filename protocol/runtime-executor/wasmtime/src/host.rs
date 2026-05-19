// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! This module defines `HostContext`, which provides logic required for host execution.

use std::cell::Cell;
pub use wasmtime::Caller;

use sp_wasm_interface::{FunctionContext, Pointer, WordSize};

use crate::{store_data::StoreData, util};

thread_local! {
    static CURRENT_CALLER: Cell<*mut ()> = const { Cell::new(std::ptr::null_mut()) };
}

struct CurrentCallerGuard<'a> {
    cell: &'a Cell<*mut ()>,
    previous: *mut (),
}

impl Drop for CurrentCallerGuard<'_> {
    fn drop(&mut self) {
        self.cell.set(self.previous);
    }
}

/// A `HostContext` implements `FunctionContext` for making host calls from a Wasmtime
/// runtime. The `HostContext` exists only for the lifetime of the call and borrows state from
/// a longer-living `HostState`.
pub(crate) struct HostContext<'a> {
    pub(crate) caller: Caller<'a, StoreData>,
}

pub(crate) fn with_host_context<R>(
    host_context: &mut HostContext<'_>,
    callback: impl FnOnce(&mut dyn FunctionContext) -> R,
) -> R {
    CURRENT_CALLER.with(|cell| {
        let _guard = CurrentCallerGuard {
            cell,
            previous: cell.replace(&mut host_context.caller as *mut _ as *mut ()),
        };
        callback(host_context)
    })
}

/// Runs `callback` with the active Wasmtime caller for the current host call.
///
/// This is the Gear-local replacement for the caller accessor that used to live in the custom
/// Polkadot SDK fork. It is intentionally scoped to the Wasmtime executor.
pub fn with_caller_mut<R>(
    _context: &mut dyn FunctionContext,
    callback: impl FnOnce(&mut Caller<'_, StoreData>) -> R,
) -> R {
    CURRENT_CALLER.with(|cell| {
        let ptr = cell.get();
        assert!(
            !ptr.is_null(),
            "Wasmtime caller is only available during a host call"
        );
        let caller = unsafe { &mut *(ptr as *mut Caller<'_, StoreData>) };
        callback(caller)
    })
}

impl<'a> FunctionContext for HostContext<'a> {
    fn read_memory_into(
        &self,
        address: Pointer<u8>,
        dest: &mut [u8],
    ) -> sp_wasm_interface::Result<()> {
        util::read_memory_into(&self.caller, address, dest).map_err(|e| e.to_string())
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> sp_wasm_interface::Result<()> {
        util::write_memory_from(&mut self.caller, address, data).map_err(|e| e.to_string())
    }

    fn allocate_memory(&mut self, size: WordSize) -> sp_wasm_interface::Result<Pointer<u8>> {
        util::allocate_memory(&mut self.caller, size).map_err(|e| e.to_string())
    }

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> sp_wasm_interface::Result<()> {
        util::deallocate_memory(&mut self.caller, ptr).map_err(|e| e.to_string())
    }

    fn register_panic_error_message(&mut self, message: &str) {
        self.caller
            .data_mut()
            .host_state_mut()
            .expect("host state is not empty when calling a function in wasm; qed")
            .panic_message = Some(message.to_owned());
    }
}
