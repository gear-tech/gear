// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Demo program showing the gas delta between two ways to verify an
//! sr25519 signature from inside a Gear program:
//!
//! - [`Mode::Wasm`]   — uses the `schnorrkel` crate compiled into the
//!                      program's own WASM. Every curve25519 scalar op is
//!                      interpreted op-by-op by the host runtime.
//! - [`Mode::Syscall`] — calls `gcore::crypto::sr25519_verify`, which
//!                      dispatches to a native implementation on the host
//!                      (`sp_core::sr25519::Pair::verify`) via the new
//!                      `gr_sr25519_verify` syscall.
//!
//! The two modes share identical inputs; only the compute path differs.
//! Pair this program with the gtest in `pallets/gear/src/tests/` (or run
//! manually in `gtest::System`) to measure the gas delta.

#![no_std]

use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm;

/// Verification-path selector. Sent as the first byte of the request.
#[derive(Debug, Clone, Copy, Encode, Decode, Eq, PartialEq)]
pub enum Mode {
    /// Verify using the `schnorrkel` crate compiled into the program WASM.
    Wasm,
    /// Verify via the `gr_sr25519_verify` syscall (native on the host).
    Syscall,
}

/// Full verification request — mode + the sr25519 triple to check.
#[derive(Debug, Clone, Encode, Decode)]
pub struct VerifyRequest {
    /// Which path to use.
    pub mode: Mode,
    /// 32-byte sr25519 public key.
    pub pk: [u8; 32],
    /// Message bytes that were signed.
    pub msg: alloc::vec::Vec<u8>,
    /// 64-byte sr25519 signature.
    pub sig: [u8; 64],
}

/// Reply shape: `1u8` on valid, `0u8` on invalid.
pub type VerifyReply = u8;

extern crate alloc;
