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

use super::{StoreData, store};
use wasmtime::Caller;

pub(crate) struct HostContext<'a> {
    pub(crate) caller: Caller<'a, StoreData>,
}

impl HostContext<'_> {
    pub(crate) fn allocate_memory(&mut self, size: u32) -> Result<u32, String> {
        store::allocate_memory(&mut self.caller, size)
    }

    pub(crate) fn deallocate_memory(&mut self, ptr: u32) -> Result<(), String> {
        store::deallocate_memory(&mut self.caller, ptr)
    }

    #[allow(unused)]
    pub(crate) fn register_panic_error_message(&mut self, message: &str) {
        self.caller
            .data_mut()
            .host_state_mut()
            .expect("host state is initialized before wasm calls; qed")
            .panic_message = Some(message.to_owned());
    }
}
