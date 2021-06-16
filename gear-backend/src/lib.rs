#![no_std]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;

#[cfg(feature = "wasmtime")]
pub mod wasmtime;