// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(not(feature = "wasm-wrapper"))]
mod wasm;

#[cfg(not(feature = "wasm-wrapper"))]
pub(crate) use wasm::DATA;

#[cfg(feature = "wasm-wrapper")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "wasm-wrapper")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

mod arg;
mod builder;
mod call;
mod scheme;

pub use arg::Arg;
pub use builder::Calls;
pub use call::Call;
pub use scheme::*;
