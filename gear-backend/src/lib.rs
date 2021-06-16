//! Crate provides support for wasm runtime.

#![no_std]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;

#[cfg(feature = "wasmtime")]
pub mod wasmtime;