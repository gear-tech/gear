//! Crate provides support for wasm runtime.

#![no_std]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;

cfg_if::cfg_if! {
    if #[cfg(feature = "wasmtime_backend")] {
        pub mod wasmtime;
        pub use crate::wasmtime::env::Environment;
    } else if #[cfg(feature = "wasmi_backend")] {
        pub mod wasmi;
        pub use crate::wasmi::env::Environment;
    }
}

mod funcs;
