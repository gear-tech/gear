// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Runtime interface for gear node

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
pub use gear_sandbox_host::sandbox::{SandboxBackend, env::Instantiate};
use sp_runtime_interface::{Pointer, runtime_interface};
use sp_wasm_interface::HostPointer;

#[cfg(feature = "host-api")]
pub mod detail;

#[cfg(feature = "host-api")]
pub use detail::init;

/// Wasm-only interface that provides functions for interacting with the sandbox.
#[runtime_interface(wasm_only)]
#[cfg_attr(not(feature = "host-api"), allow(unreachable_code, unused_variables))]
pub trait Sandbox {
    /// Instantiate a new sandbox instance with the given `wasm_code`.
    fn instantiate(
        &mut self,
        dispatch_thunk_id: u32,
        wasm_code: &[u8],
        raw_env_def: &[u8],
        state_ptr: Pointer<u8>,
    ) -> u32 {
        #[cfg(feature = "host-api")]
        return detail::instantiate(
            *self,
            dispatch_thunk_id,
            wasm_code,
            raw_env_def,
            state_ptr,
            Instantiate::Version1,
        );

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    /// Instantiate a new sandbox instance with the given `wasm_code`.
    #[version(2)]
    fn instantiate(
        &mut self,
        dispatch_thunk_id: u32,
        wasm_code: &[u8],
        raw_env_def: &[u8],
        state_ptr: Pointer<u8>,
    ) -> u32 {
        #[cfg(feature = "host-api")]
        return detail::instantiate(
            *self,
            dispatch_thunk_id,
            wasm_code,
            raw_env_def,
            state_ptr,
            Instantiate::Version2,
        );

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
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
        #[cfg(feature = "host-api")]
        return detail::invoke(
            *self,
            instance_idx,
            function,
            args,
            return_val_ptr,
            return_val_len,
            state_ptr,
        );

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    /// Create a new memory instance with the given `initial` and `maximum` size.
    fn memory_new(&mut self, initial: u32, maximum: u32) -> u32 {
        #[cfg(feature = "host-api")]
        return detail::memory_new(*self, initial, maximum);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    /// Get the memory starting at `offset` from the instance with `memory_idx` into the buffer.
    fn memory_get(
        &mut self,
        memory_idx: u32,
        offset: u32,
        buf_ptr: Pointer<u8>,
        buf_len: u32,
    ) -> u32 {
        #[cfg(feature = "host-api")]
        return detail::memory_get(*self, memory_idx, offset, buf_ptr, buf_len);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    /// Set the memory in the given `memory_idx` to the given value at `offset`.
    fn memory_set(
        &mut self,
        memory_idx: u32,
        offset: u32,
        val_ptr: Pointer<u8>,
        val_len: u32,
    ) -> u32 {
        #[cfg(feature = "host-api")]
        return detail::memory_set(*self, memory_idx, offset, val_ptr, val_len);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    /// Teardown the memory instance with the given `memory_idx`.
    fn memory_teardown(&mut self, memory_idx: u32) {
        #[cfg(feature = "host-api")]
        return detail::memory_teardown(*self, memory_idx);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    /// Teardown the sandbox instance with the given `instance_idx`.
    fn instance_teardown(&mut self, instance_idx: u32) {
        #[cfg(feature = "host-api")]
        return detail::instance_teardown(*self, instance_idx);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
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
        #[cfg(feature = "host-api")]
        return detail::get_global_val(*self, instance_idx, name);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    /// Set the value of a global with the given `name`. The sandbox is determined by the given
    /// `instance_idx`.
    fn set_global_val(
        &mut self,
        instance_idx: u32,
        name: &str,
        value: sp_wasm_interface::Value,
    ) -> u32 {
        #[cfg(feature = "host-api")]
        return detail::set_global_val(*self, instance_idx, name, value);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    fn memory_grow(&mut self, memory_idx: u32, size: u32) -> u32 {
        #[cfg(feature = "host-api")]
        return detail::memory_grow(*self, memory_idx, size);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    fn memory_size(&mut self, memory_idx: u32) -> u32 {
        #[cfg(feature = "host-api")]
        return detail::memory_size(*self, memory_idx);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    fn get_buff(&mut self, memory_idx: u32) -> HostPointer {
        #[cfg(feature = "host-api")]
        return detail::get_buff(*self, memory_idx);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }

    fn get_instance_ptr(&mut self, instance_id: u32) -> HostPointer {
        #[cfg(feature = "host-api")]
        return detail::get_instance_ptr(*self, instance_id);

        #[cfg(not(feature = "host-api"))]
        unreachable!("`host-api` feature is disabled");
    }
}
