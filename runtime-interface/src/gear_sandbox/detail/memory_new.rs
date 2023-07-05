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

pub fn method(self_: &mut dyn FunctionContext, initial: u32, maximum: u32) -> u32 {
    let mut method_result = u32::MAX;

    sp_wasm_interface::with_caller_mut(self_, |caller| {
        trace("memory_new", caller);

        let data_ptr: *const _ = caller.data();
        method_result = SANDBOXES.with(|sandboxes| {
            sandboxes
                .borrow_mut()
                .get(data_ptr as usize)
                .new_memory(initial, maximum)
                .map_err(|e| e.to_string())
                .expect("Failed to create new memory with sandbox")
        });
    });

    method_result
}
