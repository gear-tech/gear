// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! A crate that provides means of executing/dispatching calls into the runtime.
//!
//! There are a few responsibilities of this crate at the moment:
//!
//! - It provides an implementation of a common entrypoint for calling into the runtime, both
//!   wasm and compiled.
//! - It defines the environment for the wasm execution, namely the host functions that are to be
//!   provided into the wasm runtime module.
//! - It also provides the required infrastructure for executing the current wasm runtime (specified
//!   by the current value of `:code` in the provided externalities), i.e. interfacing with
//!   wasm engine used, instance cache.

#![warn(missing_docs)]

#[macro_use]
mod executor;
mod wasm_runtime;

pub use codec::Codec;
#[allow(deprecated)]
pub use executor::NativeElseWasmExecutor;
pub use executor::{NativeExecutionDispatch, WasmExecutor, with_externalities_safe};
#[doc(hidden)]
pub use sp_core::traits::Externalities;
pub use sp_version::{NativeVersion, RuntimeVersion};
#[doc(hidden)]
pub use sp_wasm_interface;
pub use sp_wasm_interface::HostFunctions;
pub use wasm_runtime::{WasmExecutionMethod, read_embedded_version};

pub use sc_executor_common::{
    error,
    wasm_runtime::{DEFAULT_HEAP_ALLOC_PAGES, DEFAULT_HEAP_ALLOC_STRATEGY, HeapAllocStrategy},
};
pub use sc_executor_wasmtime::{
    Caller as WasmtimeCaller, InstantiationStrategy as WasmtimeInstantiationStrategy, StoreData,
    util as wasmtime_util, with_caller_mut,
};

/// Wasmtime caller type used by Gear's sandbox runtime interface.
pub type Caller<'a> = WasmtimeCaller<'a, StoreData>;

/// Extracts the runtime version of a given runtime code.
pub trait RuntimeVersionOf {
    /// Extract [`RuntimeVersion`] of the given `runtime_code`.
    fn runtime_version(
        &self,
        ext: &mut dyn Externalities,
        runtime_code: &sp_core::traits::RuntimeCode,
    ) -> error::Result<RuntimeVersion>;
}
