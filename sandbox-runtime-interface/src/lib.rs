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
	use sp_wasm_interface::{wasmtime::{self, Func, Val, AsContext}, Caller, StoreData, Pointer, WordSize};
	use once_cell::unsync::Lazy;

	// The sandbox store is inside of a Option<Box<..>>> so that we can temporarily borrow it.
	pub(super) struct SandboxStore(pub(super) Box<sandbox::Store<wasmtime::Func>>);

	// There are a bunch of `Rc`s within the sandbox store, however we only manipulate
	// those within one thread so this should be safe.
	unsafe impl Send for SandboxStore {}

	pub(super) type StoreBox = Box<sandbox::Store<wasmtime::Func>>;

	pub(super) static mut SANDBOX_STORE: Lazy<StoreBox> = Lazy::new(|| {
		Box::new(sandbox::Store::new(
			sandbox::SandboxBackend::TryWasmer,
		))
	});

	pub(super) struct SandboxContext<'a, 'b> {
		pub(super) caller: &'a mut Caller<'b, StoreData>,
		pub(super) dispatch_thunk: Func,
		/// Custom data to propagate it in supervisor export functions
		pub(super) state: u32,
	}
	
	impl<'a, 'b> sandbox::SandboxContext for SandboxContext<'a, 'b> {
		fn invoke(
			&mut self,
			invoke_args_ptr: Pointer<u8>,
			invoke_args_len: WordSize,
			func_idx: sandbox::SupervisorFuncIndex,
		) -> gear_sandbox_native::error::Result<i64> {
			let mut ret_vals = [Val::null()];
			let result = self.dispatch_thunk.call(
				&mut self.caller,
				&[
					Val::I32(u32::from(invoke_args_ptr) as i32),
					Val::I32(invoke_args_len as i32),
					Val::I32(self.state as i32),
					Val::I32(usize::from(func_idx) as i32),
				],
				&mut ret_vals,
			);
	
			match result {
				Ok(()) =>
					if let Some(ret_val) = ret_vals[0].i64() {
						Ok(ret_val)
					} else {
						Err("Supervisor function returned unexpected result!".into())
					},
				Err(err) => Err(err.to_string().into()),
			}
		}
	
		// fn supervisor_context(&mut self) -> &mut dyn FunctionContext {
		// 	self.host_context
		// }

		fn read_memory_into(
			&self,
			address: Pointer<u8>,
			dest: &mut [u8],
		) -> sp_wasm_interface::Result<()> {
			let memory = self.caller.as_context().data().memory().data(&self.caller);

			let range = gear_sandbox_native::util::checked_range(address.into(), dest.len(), memory.len())
				.ok_or_else(|| String::from("memory read is out of bounds"))?;
			dest.copy_from_slice(&memory[range]);
			Ok(())
		}
	
		fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> sp_wasm_interface::Result<()> {
			let memory = self.caller.as_context().data().memory();
			let memory = memory.data_mut(&mut self.caller);

			let range = gear_sandbox_native::util::checked_range(address.into(), data.len(), memory.len())
				.ok_or_else(|| String::from("memory write is out of bounds"))?;
			memory[range].copy_from_slice(data);
			Ok(())
		}
	
		fn allocate_memory(&mut self, size: WordSize) -> sp_wasm_interface::Result<Pointer<u8>> {
			let memory = self.caller.data().memory();
			let (memory, data) = memory.data_and_store_mut(&mut self.caller);
			data.host_state_mut()
				.expect("host state is not empty when calling a function in wasm; qed")
				.allocator
				.allocate(memory, size)
				.map_err(|e| e.to_string())
		}
	
		fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> sp_wasm_interface::Result<()> {
			let memory = self.caller.data().memory();
			let (memory, data) = memory.data_and_store_mut(&mut self.caller);
			data.host_state_mut()
				.expect("host state is not empty when calling a function in wasm; qed")
				.allocator
				.deallocate(memory, ptr)
				.map_err(|e| e.to_string())
		}
	}
}

/// Wasm-only interface that provides functions for interacting with the sandbox.
#[runtime_interface(wasm_only)]
pub trait Sandbox {
	/// Instantiate a new sandbox instance with the given `wasm_code`.
	fn instantiate(
		&mut self,
		dispatch_thunk_id: u32,
		wasm_code: &[u8],
		raw_env_def: &[u8],
		state_ptr: Pointer<u8>,
	) -> u32 {
		use gear_sandbox_native::sandbox as sandbox;
		use sp_wasm_interface::wasmtime::AsContextMut;

		struct Context<'a> {
			dispatch_thunk_id: u32,
			wasm_code: &'a [u8],
			raw_env_def: &'a [u8],
			state_ptr: Pointer<u8>,
			store: &'a mut impl_::StoreBox,
			result: u32,
		}

		let mut context = Context {
			dispatch_thunk_id,
			wasm_code,
			raw_env_def,
			state_ptr,
			store: unsafe { &mut impl_::SANDBOX_STORE },
			result: 0,
		};
		let context_ptr: *mut Context = &mut context;

		self.with_caller_mut(context_ptr as *mut (), |context_ptr, caller| {
			let context_ptr: *mut Context = context_ptr.cast();
			let context: &mut Context = unsafe { context_ptr.as_mut().expect("") };

			// Extract a dispatch thunk from the instance's table by the specified index.
			let dispatch_thunk = {
				let table = caller
					.data()
					.table
					.ok_or("Runtime doesn't have a table; sandbox is unavailable")
					.expect("Failed to instantiate a new sandbox");
				let table_item = table.get(caller.as_context_mut(), context.dispatch_thunk_id);

				*table_item
					.ok_or("dispatch_thunk_id is out of bounds")
					.expect("Failed to instantiate a new sandbox")
					.funcref()
					.ok_or("dispatch_thunk_idx should be a funcref")
					.expect("Failed to instantiate a new sandbox")
					.ok_or("dispatch_thunk_idx should point to actual func")
					.expect("Failed to instantiate a new sandbox")
			};

			let guest_env = match sandbox::GuestEnvironment::decode(context.store.as_ref(), context.raw_env_def) {
				Ok(guest_env) => guest_env,
				Err(_) => {
					context.result = sandbox::env::ERR_MODULE as u32;
					return;
				}
			};

			// let mut store = self
			// 	.host_state_mut()
			// 	.sandbox_store
			// 	.0
			// 	.take()
			// 	.expect("sandbox store is only empty when borrowed");

			// Catch any potential panics so that we can properly restore the sandbox store
			// which we've destructively borrowed.
			let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
				context.store.as_mut().instantiate(
					context.wasm_code,
					guest_env,
					&mut impl_::SandboxContext { caller, dispatch_thunk, state: context.state_ptr.into() },
				)
			}));

			// self.host_state_mut().sandbox_store.0 = Some(store);

			let result = match result {
				Ok(result) => result,
				Err(error) => std::panic::resume_unwind(error),
			};

			let instance_idx_or_err_code = match result {
				Ok(instance) => instance.register(context.store.as_mut(), dispatch_thunk),
				Err(sandbox::InstantiationError::StartTrapped) => sandbox::env::ERR_EXECUTION,
				Err(_) => sandbox::env::ERR_MODULE,
			};

			context.result = instance_idx_or_err_code as u32
		});

		context.result
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
		type Context<'a> = (&'a mut impl_::StoreBox, u32, u32, u32);
		let mut context: Context = (unsafe { &mut impl_::SANDBOX_STORE }, initial, maximum, 0);
		let context_ptr: *mut Context = &mut context;
        self.with_caller_mut(context_ptr as *mut (), |context_ptr, _caller| {
			let context_ptr: *mut Context = context_ptr.cast();
			let context: &mut Context = unsafe { context_ptr.as_mut().expect("") };
			let (store, initial, maximum, result) = context;
            *result = store.as_mut().new_memory(*initial, *maximum).map_err(|e| e.to_string())
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
		struct Context<'a> {
			instance_idx: u32,
			name: &'a str,
			store: &'a mut impl_::StoreBox,
			result: Option<sp_wasm_interface::Value>,
		}

		let mut context = Context {
			instance_idx,
			name,
			store: unsafe { &mut impl_::SANDBOX_STORE },
			result: None,
		};
		let context_ptr: *mut Context = &mut context;

		self.with_caller_mut(context_ptr as *mut (), |context_ptr, _caller| {
			let context_ptr: *mut Context = context_ptr.cast();
			let context: &mut Context = unsafe { context_ptr.as_mut().expect("") };

			context.result = context.store.as_mut()
				.instance(context.instance_idx)
				.map(|i| i.get_global_val(context.name))
				.map_err(|e| e.to_string())
				.expect("Failed to get global from sandbox");
		});

		context.result
	}

    /// Set the value of a global with the given `name`. The sandbox is determined by the given
	/// `instance_idx`.
	fn set_global_val(
		&mut self,
		instance_idx: u32,
		name: &str,
		value: sp_wasm_interface::Value,
	) -> u32 {
		use gear_sandbox_native::sandbox as sandbox;

		struct Context<'a> {
			instance_idx: u32,
			name: &'a str,
			value: sp_wasm_interface::Value,
			store: &'a mut impl_::StoreBox,
			result: u32,
		}

		let mut context = Context {
			instance_idx,
			name,
			value,
			store: unsafe { &mut impl_::SANDBOX_STORE },
			result: u32::MAX,
		};
		let context_ptr: *mut Context = &mut context;

		self.with_caller_mut(context_ptr as *mut (), |context_ptr, _caller| {
			let context_ptr: *mut Context = context_ptr.cast();
			let context: &mut Context = unsafe { context_ptr.as_mut().expect("") };

			let instance_idx = context.instance_idx;
            log::trace!(target: "gear-sandbox", "set_global_val, instance_idx={instance_idx}");

			let instance = context.store
				.instance(instance_idx)
				.map_err(|e| e.to_string())
				.expect("Failed to set global in sandbox");

			let result = instance.set_global_val(context.name, context.value);

			log::trace!(target: "gear-sandbox", "set_global_val, name={}, value={:?}, result={result:?}", context.name, context.value);
			context.result = match result {
				Ok(None) => sandbox::env::ERROR_GLOBALS_NOT_FOUND,
				Ok(Some(_)) => sandbox::env::ERROR_GLOBALS_OK,
				Err(_) => sandbox::env::ERROR_GLOBALS_OTHER,
			};
        });

        context.result
	}

	fn memory_grow(&mut self, memory_idx: u32, size: u32) -> u32 {
		use gear_sandbox_native::util::MemoryTransfer;

        struct Context<'a> {
			memory_idx: u32,
			size: u32,
			store: &'a mut impl_::StoreBox,
			result: u32,
		}

		let mut context = Context {
			memory_idx,
			size,
			store: unsafe { &mut impl_::SANDBOX_STORE },
			result: u32::MAX,
		};
		let context_ptr: *mut Context = &mut context;

		self.with_caller_mut(context_ptr as *mut (), |context_ptr, _caller| {
			let context_ptr: *mut Context = context_ptr.cast();
			let context: &mut Context = unsafe { context_ptr.as_mut().expect("") };

			let mut m = context.store.memory(context.memory_idx)
				.expect("Failed to grow memory: cannot get backend memory");
			context.result = m.memory_grow(context.size).expect("Failed to grow memory: cannot get backend memory");
        });

        context.result
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
