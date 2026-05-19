// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]

use core::array::IntoIter;
use gstd::ActorId;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Decode, Encode)]
pub struct InputArgs {
    pub approver_first: ActorId,
    pub approver_second: ActorId,
    pub approver_third: ActorId,
}

impl InputArgs {
    pub fn from_two(first: impl Into<[u8; 32]>, second: impl Into<[u8; 32]>) -> Self {
        Self {
            approver_first: first.into().into(),
            approver_second: second.into().into(),
            approver_third: ActorId::zero(),
        }
    }

    pub fn iter(&self) -> IntoIter<&ActorId, 3> {
        [
            &self.approver_first,
            &self.approver_second,
            &self.approver_third,
        ]
        .into_iter()
    }
}

#[cfg(not(feature = "std"))]
mod wasm;
