// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

use gstd::Vec;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct ResendPushData {
    pub destination: gstd::ActorId,
    pub start: Option<u32>,
    // flag indicates if the end index is included
    pub end: Option<(u32, bool)>,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum RelayCall {
    Resend(gstd::ActorId),
    ResendWithGas(gstd::ActorId, u64),
    ResendPush(Vec<ResendPushData>),
    Rereply,
    RereplyWithGas(u64),
    RereplyPush,
}

#[cfg(not(feature = "std"))]
mod wasm;
