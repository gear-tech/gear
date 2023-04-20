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
    memory_idx: u32,
    offset: u32,
    buf_ptr: Pointer<u8>,
    buf_len: u32,
) -> u32 {
    use gear_sandbox_native::util::MemoryTransfer;

    struct Context<'a> {
        store: &'a mut Store,
        result: u32,
        memory_idx: u32,
        offset: u32,
        buf_ptr: Pointer<u8>,
        buf_len: u32,
    }

    let mut context = Context {
        store: unsafe { &mut SANDBOX_STORE },
        result: u32::MAX,
        memory_idx,
        offset,
        buf_ptr,
        buf_len,
    };
    let context_ptr: *mut Context = &mut context;

    self_.with_caller_mut(context_ptr as *mut (), |context_ptr, caller| {
        let context_ptr: *mut Context = context_ptr.cast();
        let context: &mut Context = unsafe { context_ptr.as_mut().expect("memory_get; set above") };

        trace("memory_get", caller);

        let data_ptr: *const _ = caller.data();
        let sandboxed_memory = context
            .store
            .get(data_ptr as u64)
            .memory(context.memory_idx)
            .expect("sandboxed memory not found");

        let len = context.buf_len as usize;

        let buffer = match sandboxed_memory.read(Pointer::new(context.offset), len) {
            Err(_) => {
                context.result = sandbox_env::env::ERR_OUT_OF_BOUNDS;
                return;
            }
            Ok(buffer) => buffer,
        };

        context.result = match write_memory(caller, context.buf_ptr, &buffer) {
            Ok(_) => sandbox_env::env::ERR_OK,
            Err(_) => sandbox_env::env::ERR_OUT_OF_BOUNDS,
        };
    });

    context.result
}
