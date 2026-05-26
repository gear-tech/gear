// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]

use gstd::ActorId;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;

// Re-exports for testing
pub fn default_wait_up_to_duration() -> u32 {
    gstd::Config::wait_up_to()
}

#[derive(Debug, Encode, Decode)]
pub enum Command {
    Wake,
    WaitLost(ActorId),
    SendTimeout(ActorId, u32),
    JoinTimeout(ActorId, u32, u32),
    SelectTimeout(ActorId, u32, u32),
}
