// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Defines a `WasmRuntime` that uses the Wasmtime JIT to execute.
//!
//! You can choose a profiling strategy at runtime with
//! environment variable `WASMTIME_PROFILING_STRATEGY`:
//!
//! | `WASMTIME_PROFILING_STRATEGY` | Effect |
//! |-------------|-------------------------|
//! | undefined   | No profiling            |
//! | `"jitdump"` | jitdump profiling       |
//! | other value | No profiling (warning)  |

mod host;
mod host_state;
mod imports;
mod instance_wrapper;
mod memory_wrapper;
mod runtime;
mod store_data;
pub mod util;

#[cfg(test)]
mod tests;

pub use host::{Caller, with_caller_mut};
pub use host_state::HostState;
pub use runtime::{
    Config, DeterministicStackLimit, InstantiationStrategy, Semantics, WasmtimeRuntime,
    create_runtime, create_runtime_from_artifact, create_runtime_from_artifact_bytes,
    prepare_runtime_artifact,
};
pub use sc_executor_common::{
    runtime_blob::RuntimeBlob,
    wasm_runtime::{HeapAllocStrategy, WasmModule},
};
pub use store_data::StoreData;
