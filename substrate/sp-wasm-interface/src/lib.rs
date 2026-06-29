// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Types and traits for interfacing between the host and the wasm runtime.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use core::{iter::Iterator, marker::PhantomData, result};
pub use sp_wasm_interface_common::{
    self as common, HostPointer, IntoValue, MemoryId, Pointer, PointerType, ReturnValue, Signature,
    TryFromValue, Value, ValueType, WordSize,
};

if_wasmtime_is_enabled! {
    mod host_state;
    pub use host_state::HostState;

    mod store_data;
    pub use store_data::StoreData;

    mod memory_wrapper;
    pub use memory_wrapper::MemoryWrapper;

    pub mod util;
}

#[cfg(not(all(feature = "std", feature = "wasmtime")))]
pub struct StoreData;

#[cfg(not(all(feature = "std", feature = "wasmtime")))]
#[macro_export]
macro_rules! if_wasmtime_is_enabled {
    ($($token:tt)*) => {};
}

#[cfg(all(feature = "std", feature = "wasmtime"))]
#[macro_export]
macro_rules! if_wasmtime_is_enabled {
    ($($token:tt)*) => {
        $($token)*
    }
}

if_wasmtime_is_enabled! {
    // Reexport wasmtime so that its types are accessible from the procedural macro.
    pub use wasmtime;

    pub use wasmtime::anyhow;
}

/// Result type used by traits in this crate.
#[cfg(feature = "std")]
pub type Result<T> = result::Result<T, String>;
#[cfg(not(feature = "std"))]
pub type Result<T> = result::Result<T, &'static str>;

/// Provides `Sealed` trait to prevent implementing trait `PointerType` and `WasmTy` outside of this
/// crate.
mod private {
    pub trait Sealed {}

    impl Sealed for u8 {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
    impl Sealed for u64 {}

    impl Sealed for i32 {}
    impl Sealed for i64 {}
}

/// A trait that requires `RefUnwindSafe` when `feature = std`.
#[cfg(feature = "std")]
pub trait MaybeRefUnwindSafe: std::panic::RefUnwindSafe {}
#[cfg(feature = "std")]
impl<T: std::panic::RefUnwindSafe> MaybeRefUnwindSafe for T {}

/// A trait that requires `RefUnwindSafe` when `feature = std`.
#[cfg(not(feature = "std"))]
pub trait MaybeRefUnwindSafe {}
#[cfg(not(feature = "std"))]
impl<T> MaybeRefUnwindSafe for T {}

/// Something that provides a function implementation on the host for a wasm function.
pub trait Function: MaybeRefUnwindSafe + Send + Sync {
    /// Returns the name of this function.
    fn name(&self) -> &str;
    /// Returns the signature of this function.
    fn signature(&self) -> Signature;
    /// Execute this function with the given arguments.
    fn execute(
        &self,
        context: &mut dyn FunctionContext,
        args: &mut dyn Iterator<Item = Value>,
    ) -> Result<Option<Value>>;
}

impl PartialEq for dyn Function {
    fn eq(&self, other: &Self) -> bool {
        other.name() == self.name() && other.signature() == self.signature()
    }
}

#[cfg(not(all(feature = "std", feature = "wasmtime")))]
pub struct Caller<'a, T>(PhantomData<&'a T>);

#[cfg(all(feature = "std", feature = "wasmtime"))]
pub use wasmtime::Caller;

/// Context used by `Function` to interact with the allocator and the memory of the wasm instance.
pub trait FunctionContext {
    /// Read memory from `address` into a vector.
    fn read_memory(&self, address: Pointer<u8>, size: WordSize) -> Result<Vec<u8>> {
        let mut vec = vec![0; size as usize];
        self.read_memory_into(address, &mut vec)?;
        Ok(vec)
    }
    /// Read memory into the given `dest` buffer from `address`.
    fn read_memory_into(&self, address: Pointer<u8>, dest: &mut [u8]) -> Result<()>;
    /// Write the given data at `address` into the memory.
    fn write_memory(&mut self, address: Pointer<u8>, data: &[u8]) -> Result<()>;
    /// Allocate a memory instance of `size` bytes.
    fn allocate_memory(&mut self, size: WordSize) -> Result<Pointer<u8>>;
    /// Deallocate a given memory instance.
    fn deallocate_memory(&mut self, ptr: Pointer<u8>) -> Result<()>;
    /// Registers a panic error message within the executor.
    ///
    /// This is meant to be used in situations where the runtime
    /// encounters an unrecoverable error and intends to panic.
    ///
    /// Panicking in WASM is done through the [`unreachable`](https://webassembly.github.io/spec/core/syntax/instructions.html#syntax-instr-control)
    /// instruction which causes an unconditional trap and immediately aborts
    /// the execution. It does not however allow for any diagnostics to be
    /// passed through to the host, so while we do know that *something* went
    /// wrong we don't have any direct indication of what *exactly* went wrong.
    ///
    /// As a workaround we use this method right before the execution is
    /// actually aborted to pass an error message to the host so that it
    /// can associate it with the next trap, and return that to the caller.
    ///
    /// A WASM trap should be triggered immediately after calling this method;
    /// otherwise the error message might be associated with a completely
    /// unrelated trap.
    ///
    /// It should only be called once, however calling it more than once
    /// is harmless and will overwrite the previously set error message.
    fn register_panic_error_message(&mut self, message: &str);
}

if_wasmtime_is_enabled! {
    /// A trait used to statically register host callbacks with the WASM executor,
    /// so that they can be called from within the runtime with minimal overhead.
    ///
    /// This is used internally to interface the wasmtime-based executor with the
    /// host functions' definitions generated through the runtime interface macro,
    /// and is not meant to be used directly.
    pub trait HostFunctionRegistry {
        type State: 'static;
        type Error;
        type FunctionContext: FunctionContext;

        /// Wraps the given `caller` in a type which implements `FunctionContext`
        /// and calls the given `callback`.
        fn with_function_context<R>(
            caller: wasmtime::Caller<Self::State>,
            callback: impl FnOnce(&mut dyn FunctionContext) -> R,
        ) -> R;

        /// Registers a given host function with the WASM executor.
        ///
        /// The function has to be statically callable, and all of its arguments
        /// and its return value have to be compatible with WASM FFI.
        fn register_static<Params, Results>(
            &mut self,
            fn_name: &str,
            func: impl wasmtime::IntoFunc<Self::State, Params, Results> + 'static,
        ) -> core::result::Result<(), Self::Error>;
    }
}

/// Something that provides implementations for host functions.
pub trait HostFunctions: 'static + Send + Sync {
    /// Returns the host functions `Self` provides.
    fn host_functions() -> Vec<&'static dyn Function>;

    if_wasmtime_is_enabled! {
        /// Statically registers the host functions.
        fn register_static<T>(registry: &mut T) -> core::result::Result<(), T::Error>
        where
            T: HostFunctionRegistry;
    }
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl HostFunctions for Tuple {
    #[allow(clippy::let_and_return)]
    fn host_functions() -> Vec<&'static dyn Function> {
        let mut host_functions = Vec::new();

        for_tuples!( #( host_functions.extend(Tuple::host_functions()); )* );

        host_functions
    }

    #[cfg(all(feature = "std", feature = "wasmtime"))]
    fn register_static<T>(registry: &mut T) -> core::result::Result<(), T::Error>
    where
        T: HostFunctionRegistry,
    {
        for_tuples!(
            #( Tuple::register_static(registry)?; )*
        );

        Ok(())
    }
}

/// A wrapper which merges two sets of host functions, and allows the second set to override
/// the host functions from the first set.
pub struct ExtendedHostFunctions<Base, Overlay> {
    phantom: PhantomData<(Base, Overlay)>,
}

impl<Base, Overlay> HostFunctions for ExtendedHostFunctions<Base, Overlay>
where
    Base: HostFunctions,
    Overlay: HostFunctions,
{
    fn host_functions() -> Vec<&'static dyn Function> {
        let mut base = Base::host_functions();
        let overlay = Overlay::host_functions();
        base.retain(|host_fn| {
            !overlay
                .iter()
                .any(|ext_host_fn| host_fn.name() == ext_host_fn.name())
        });
        base.extend(overlay);
        base
    }

    if_wasmtime_is_enabled! {
        fn register_static<T>(registry: &mut T) -> core::result::Result<(), T::Error>
        where
            T: HostFunctionRegistry,
        {
            struct Proxy<'a, T> {
                registry: &'a mut T,
                seen_overlay: std::collections::HashSet<String>,
                seen_base: std::collections::HashSet<String>,
                overlay_registered: bool,
            }

            impl<'a, T> HostFunctionRegistry for Proxy<'a, T>
            where
                T: HostFunctionRegistry,
            {
                type State = T::State;
                type Error = T::Error;
                type FunctionContext = T::FunctionContext;

                fn with_function_context<R>(
                    caller: wasmtime::Caller<Self::State>,
                    callback: impl FnOnce(&mut dyn FunctionContext) -> R,
                ) -> R {
                    T::with_function_context(caller, callback)
                }

                fn register_static<Params, Results>(
                    &mut self,
                    fn_name: &str,
                    func: impl wasmtime::IntoFunc<Self::State, Params, Results> + 'static,
                ) -> core::result::Result<(), Self::Error> {
                    if self.overlay_registered {
                        if !self.seen_base.insert(fn_name.to_owned()) {
                            log::warn!(
                                target: "extended_host_functions",
                                "Duplicate base host function: '{}'",
                                fn_name,
                            );

                            // TODO: Return an error here?
                            return Ok(())
                        }

                        if self.seen_overlay.contains(fn_name) {
                            // Was already registered when we went through the overlay, so just ignore it.
                            log::debug!(
                                target: "extended_host_functions",
                                "Overriding base host function: '{}'",
                                fn_name,
                            );

                            return Ok(())
                        }
                    } else if !self.seen_overlay.insert(fn_name.to_owned()) {
                        log::warn!(
                            target: "extended_host_functions",
                            "Duplicate overlay host function: '{}'",
                            fn_name,
                        );

                        // TODO: Return an error here?
                        return Ok(())
                    }

                    self.registry.register_static(fn_name, func)
                }
            }

            let mut proxy = Proxy {
                registry,
                seen_overlay: Default::default(),
                seen_base: Default::default(),
                overlay_registered: false,
            };

            // The functions from the `Overlay` can override those from the `Base`,
            // so `Overlay` is registered first, and then we skip those functions
            // in `Base` if they were already registered from the `Overlay`.
            Overlay::register_static(&mut proxy)?;
            proxy.overlay_registered = true;
            Base::register_static(&mut proxy)?;

            Ok(())
        }
    }
}

/// A trait for types directly usable at the WASM FFI boundary without any conversion at all.
///
/// This trait is sealed and should not be implemented downstream.
#[cfg(all(feature = "std", feature = "wasmtime"))]
pub trait WasmTy: wasmtime::WasmTy + private::Sealed {}

/// A trait for types directly usable at the WASM FFI boundary without any conversion at all.
///
/// This trait is sealed and should not be implemented downstream.
#[cfg(not(all(feature = "std", feature = "wasmtime")))]
pub trait WasmTy: private::Sealed {}

impl WasmTy for i32 {}
impl WasmTy for u32 {}
impl WasmTy for i64 {}
impl WasmTy for u64 {}
