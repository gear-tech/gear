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

//! Demo program exercising all seven crypto/hash `gr_*` syscalls.
//!
//! Accepts a SCALE-encoded [`Op`] in the incoming message payload,
//! dispatches to the matching syscall (or the pure-WASM schnorrkel
//! baseline for the sr25519 gas-delta comparison), and replies with
//! raw bytes that tests interpret per-op:
//!
//! | Op                              | Reply                                    |
//! |---------------------------------|------------------------------------------|
//! | `Sr25519Verify{Wasm,Syscall}`   | `[1u8]` valid / `[0u8]` invalid          |
//! | `Ed25519Verify`                 | `[1u8]` valid / `[0u8]` invalid          |
//! | `Secp256k1Verify`               | `[1u8]` valid / `[0u8]` invalid          |
//! | `Secp256k1Recover`              | SCALE `Option<[u8;65]>` (`[0]` or `[1, pk…]`) |
//! | `Blake2b256` / `Sha256` / `Keccak256` | 32-byte digest                      |

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

extern crate alloc;

use alloc::vec::Vec;

/// Request dispatched to the demo program's `handle()`.
#[derive(Debug, Clone, Encode, Decode)]
pub enum Op {
    /// Verify sr25519 signature by running schnorrkel inside the program
    /// WASM (no syscall). Baseline for the gas-delta comparison.
    /// `ctx` is the Schnorrkel simple signing context — must match
    /// what the off-chain signer used (typically `b"substrate"`).
    Sr25519VerifyWasm {
        pk: [u8; 32],
        ctx: Vec<u8>,
        msg: Vec<u8>,
        sig: [u8; 64],
    },
    /// Verify sr25519 signature via the `gr_sr25519_verify` syscall.
    Sr25519VerifySyscall {
        pk: [u8; 32],
        ctx: Vec<u8>,
        msg: Vec<u8>,
        sig: [u8; 64],
    },
    /// Verify ed25519 signature via the `gr_ed25519_verify` syscall.
    Ed25519Verify {
        pk: [u8; 32],
        msg: Vec<u8>,
        sig: [u8; 64],
    },
    /// Verify secp256k1 ECDSA signature via the `gr_secp256k1_verify`
    /// syscall. `msg_hash` is the pre-computed digest. When `strict`
    /// is true, high-s signatures are rejected at the ABI.
    Secp256k1Verify {
        msg_hash: [u8; 32],
        sig: [u8; 65],
        pk: [u8; 33],
        strict: bool,
    },
    /// Recover the secp256k1 public key via `gr_secp256k1_recover`.
    /// When `strict` is true, high-s signatures return `None`.
    Secp256k1Recover {
        msg_hash: [u8; 32],
        sig: [u8; 65],
        strict: bool,
    },
    /// BLAKE2b-256 via `gr_blake2b_256`.
    Blake2b256(Vec<u8>),
    /// SHA-256 via `gr_sha256`.
    Sha256(Vec<u8>),
    /// Keccak-256 (Ethereum-style) via `gr_keccak256`.
    Keccak256(Vec<u8>),
}
