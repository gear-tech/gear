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

pub fn method(self_: &mut dyn FunctionContext, instance_id: u32) -> HostPointer {
    let mut method_result: HostPointer = u32::MAX.into();

    sp_wasm_interface::with_caller_mut(self_, |caller| {
        trace("get_instance_ptr", caller);

        let data_ptr: *const _ = caller.data();
        let instance = unsafe { &mut SANDBOX_STORE }
            .get(data_ptr as u64)
            .instance(instance_id)
            .expect("Failed to get sandboxed instance");

        method_result = instance.as_ref().get_ref()
            as *const gear_sandbox_native::sandbox::SandboxInstance
            as HostPointer;
    });

    method_result
}
