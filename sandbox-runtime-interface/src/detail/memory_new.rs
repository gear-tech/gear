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
    struct Context<'a> {
        store: &'a mut Store,
        result: u32,
        initial: u32,
        maximum: u32,
    }

    let mut context = Context {
        store: unsafe { &mut SANDBOX_STORE },
        result: u32::MAX,
        initial,
        maximum,
    };
    let context_ptr: *mut Context = &mut context;

    self_.with_caller_mut(context_ptr as *mut (), |context_ptr, caller| {
        let context_ptr: *mut Context = context_ptr.cast();
        let context: &mut Context = unsafe { context_ptr.as_mut().expect("memory_new; set above") };

        trace("memory_new", caller);

        let data_ptr: *const _ = caller.data();
        context.result = context
            .store
            .get(data_ptr as u64)
            .new_memory(context.initial, context.maximum)
            .map_err(|e| e.to_string())
            .expect("Failed to create new memory with sandbox");
    });

    context.result
}
