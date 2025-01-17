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

use sp_wasm_interface::{FunctionContextToken, Pointer, StoreData, WordSize};
use wasmtime::Caller;

pub(crate) struct HostContext<'a> {
    pub(crate) caller: Caller<'a, StoreData>,
}

impl sp_wasm_interface::FunctionContext for HostContext<'_> {
    fn read_memory_into(
        &self,
        address: Pointer<u8>,
        dest: &mut [u8],
    ) -> sp_wasm_interface::Result<()> {
        sp_wasm_interface::util::read_memory_into(&self.caller, address, dest)
            .map_err(|e| e.to_string())
    }

    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> sp_wasm_interface::Result<()> {
        sp_wasm_interface::util::write_memory_from(&mut self.caller, address, data)
            .map_err(|e| e.to_string())
    }

    fn allocate_memory(&mut self, size: WordSize) -> sp_wasm_interface::Result<Pointer<u8>> {
        sp_wasm_interface::util::allocate_memory(&mut self.caller, size)
    }

    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> sp_wasm_interface::Result<()> {
        sp_wasm_interface::util::deallocate_memory(&mut self.caller, ptr)
    }

    fn register_panic_error_message(&mut self, message: &str) {
        self.caller
            .data_mut()
            .host_state_mut()
            .expect("host state is not empty when calling a function in wasm; qed")
            .panic_message = Some(message.to_owned());
    }

    fn with_caller_mut_impl(
        &mut self,
        _: FunctionContextToken,
        context: *mut (),
        callback: fn(*mut (), &mut Caller<StoreData>),
    ) {
        callback(context, &mut self.caller)
    }
}
