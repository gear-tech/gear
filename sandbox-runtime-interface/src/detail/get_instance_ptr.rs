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
    struct Context<'a> {
        store: &'a mut Store,
        result: HostPointer,
        instance_id: u32,
    }

    let mut context = Context {
        store: unsafe { &mut SANDBOX_STORE },
        result: u32::MAX.into(),
        instance_id,
    };
    let context_ptr: *mut Context = &mut context;

    self_.with_caller_mut(context_ptr as *mut (), |context_ptr, caller| {
        let context_ptr: *mut Context = context_ptr.cast();
        let context: &mut Context = unsafe { context_ptr.as_mut().expect("") };

        let data_ptr: *const _ = caller.data();
        let store_data_key = data_ptr as u64;
        {
            // logging
            let data_ptr: *const _ = caller.data();
            let caller_ptr: *mut _ = caller;
            let thread_id = std::thread::current().id();

            log::trace!(target: "gear-sandbox-runtime-interface",
                "get_instance_ptr; data = {:#x?}, caller_ptr = {:#x?}, thread_id = {:?}",
                data_ptr as u64,
                caller_ptr as u64,
                thread_id,
            );
        }

        let instance = context
            .store
            .get(store_data_key)
            .instance(context.instance_id)
            .expect("Failed to get sandboxed instance");

        context.result = instance.as_ref().get_ref()
            as *const gear_sandbox_native::sandbox::SandboxInstance
            as HostPointer;
    });

    context.result
}
