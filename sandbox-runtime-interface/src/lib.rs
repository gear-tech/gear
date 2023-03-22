// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Runtime interface for gear node

#![allow(useless_deprecated, deprecated)]
#![cfg_attr(not(feature = "std"), no_std)]

use sp_runtime_interface::{
	runtime_interface, Pointer,
};
use sp_wasm_interface::HostPointer;

#[cfg(feature = "std")]
mod impl_ {
	use gear_sandbox_native::sandbox as sandbox;
	use sp_wasm_interface::wasmtime;
	use once_cell::unsync::Lazy;

	// The sandbox store is inside of a Option<Box<..>>> so that we can temporarily borrow it.
	pub(super) struct SandboxStore(pub(super) Option<Box<sandbox::Store<wasmtime::Func>>>);

	// There are a bunch of `Rc`s within the sandbox store, however we only manipulate
	// those within one thread so this should be safe.
	unsafe impl Send for SandboxStore {}

	pub(super) static mut SANDBOX_STORE: Lazy<SandboxStore> = Lazy::new(|| {
		SandboxStore(Some(Box::new(sandbox::Store::new(
			sandbox::SandboxBackend::TryWasmer,
		))))
	});
}

/// Wasm-only interface that provides functions for interacting with the sandbox.
#[runtime_interface(wasm_only)]
pub trait Sandbox {
	/// Instantiate a new sandbox instance with the given `wasm_code`.
	fn instantiate(
		&mut self,
		dispatch_thunk: u32,
		wasm_code: &[u8],
		env_def: &[u8],
		state_ptr: Pointer<u8>,
	) -> u32 {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	/// Invoke `function` in the sandbox with `sandbox_idx`.
	fn invoke(
		&mut self,
		instance_idx: u32,
		function: &str,
		args: &[u8],
		return_val_ptr: Pointer<u8>,
		return_val_len: u32,
		state_ptr: Pointer<u8>,
	) -> u32 {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	/// Create a new memory instance with the given `initial` and `maximum` size.
	fn memory_new(&mut self, initial: u32, maximum: u32) -> u32 {
		type Context<'a> = (&'a mut impl_::SandboxStore, u32, u32, u32);
		let mut context: Context = (unsafe { &mut impl_::SANDBOX_STORE }, initial, maximum, 0);
		let context_ptr: *mut Context = &mut context;
        self.with_caller_mut(context_ptr as *mut (), |context_ptr, _caller| {
			let context_ptr: *mut Context = context_ptr.cast();
			let context: &mut Context = unsafe { context_ptr.as_mut().expect("") };
			let (store, initial, maximum, result) = context;
            *result = store.0.as_mut().expect("sandbox store is not empty").new_memory(*initial, *maximum).map_err(|e| e.to_string())
				.expect("Failed to create new memory with sandbox");
        });

		context.3
	}

	/// Get the memory starting at `offset` from the instance with `memory_idx` into the buffer.
	fn memory_get(
		&mut self,
		memory_idx: u32,
		offset: u32,
		buf_ptr: Pointer<u8>,
		buf_len: u32,
	) -> u32 {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	/// Set the memory in the given `memory_idx` to the given value at `offset`.
	fn memory_set(
		&mut self,
		memory_idx: u32,
		offset: u32,
		val_ptr: Pointer<u8>,
		val_len: u32,
	) -> u32 {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	/// Teardown the memory instance with the given `memory_idx`.
	fn memory_teardown(&mut self, memory_idx: u32) {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	/// Teardown the sandbox instance with the given `instance_idx`.
	fn instance_teardown(&mut self, instance_idx: u32) {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	/// Get the value from a global with the given `name`. The sandbox is determined by the given
	/// `instance_idx`.
	///
	/// Returns `Some(_)` when the requested global variable could be found.
	fn get_global_val(
		&mut self,
		instance_idx: u32,
		name: &str,
	) -> Option<sp_wasm_interface::Value> {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

    /// Set the value of a global with the given `name`. The sandbox is determined by the given
	/// `instance_idx`.
	fn set_global_val(
		&mut self,
		instance_idx: u32,
		name: &str,
		value: sp_wasm_interface::Value,
	) -> u32 {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	fn memory_grow(&mut self, memory_idx: u32, size: u32) -> u32 {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	fn memory_size(&mut self, memory_idx: u32) -> u32 {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	fn get_buff(&mut self, memory_idx: u32) -> HostPointer {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}

	fn get_instance_ptr(&mut self, instance_id: u32) -> HostPointer {
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
	}
}
