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

pub fn method(self_: &mut dyn FunctionContext, memory_idx: u32, size: u32) -> u32 {
    use gear_sandbox_host::util::MemoryTransfer;

    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(self_, |caller| {
        trace("memory_grow", caller);

        let data_ptr: *const _ = caller.data();
        let mut m = unsafe { &mut SANDBOX_STORE }
            .get(data_ptr as u64)
            .memory(memory_idx)
            .expect("Failed to grow memory: cannot get backend memory");

        method_result = m.memory_grow(size).expect("Failed to grow memory");
    });

    method_result
}
