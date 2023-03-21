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
        self.with_caller_mut(std::ptr::null_mut(), |_context, _caller| {
            todo!()
        });
        
        todo!()
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
