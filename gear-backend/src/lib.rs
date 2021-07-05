//! Crate provides support for wasm runtime.

#![no_std]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;

#[cfg(feature = "wasmtime_backend")]
pub mod wasmtime;

#[cfg(feature = "wasmi_backend")]
pub mod wasmi;
