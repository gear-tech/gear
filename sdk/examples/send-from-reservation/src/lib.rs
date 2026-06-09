// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Encode, Decode)]
pub enum HandleAction {
    SendToUser,
    SendToUserDelayed,
    SendToProgram { pid: [u8; 32], user: [u8; 32] },
    SendToProgramDelayed { pid: [u8; 32], user: [u8; 32] },
    ReplyToUser,
    ReplyToProgram { pid: [u8; 32], user: [u8; 32] },
    ReplyToProgramStep2([u8; 32]),
    ReceiveFromProgram([u8; 32]),
    ReceiveFromProgramDelayed([u8; 32]),
}

#[cfg(not(feature = "std"))]
mod wasm;
