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

pub fn method(self_: &mut dyn FunctionContext, instance_idx: u32, name: &str, value: Value) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(self_, |caller| {
        trace("set_global_val", caller);

        log::trace!("set_global_val, instance_idx={instance_idx}");

        let data_ptr: *const _ = caller.data();
        let instance = unsafe { &mut SANDBOX_STORE }
            .get(data_ptr as u64)
            .instance(instance_idx)
            .map_err(|e| e.to_string())
            .expect("Failed to set global in sandbox");

        let result = instance.set_global_val(name, value);

        log::trace!("set_global_val, name={name}, value={value:?}, result={result:?}",);

        method_result = match result {
            Ok(None) => sandbox_env::env::ERROR_GLOBALS_NOT_FOUND,
            Ok(Some(_)) => sandbox_env::env::ERROR_GLOBALS_OK,
            Err(_) => sandbox_env::env::ERROR_GLOBALS_OTHER,
        };
    });

    method_result
}
