// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Signer library for ethexe.
//!
//! The crate defines types and related logic for private keys, public keys types,
//! cryptographic signatures and ethereum address.
//!
//! Cryptographic instrumentary of the crate is based on secp256k1 standard
//! using [k256](https://crates.io/crates/k256) crate, but all the
//! machinery used is wrapped in the crate's types.

mod address;
mod digest;
mod keys;
mod signature;
mod signer;
mod storage;

// Exports
pub use address::Address;
pub use digest::{Digest, ToDigest};
pub use keys::{PrivateKey, PublicKey};
pub use sha3;
pub use signature::{ContractSignature, Signature, SignedData};
pub use signer::Signer;
pub use storage::{FSKeyStorage, KeyStorage, MemoryKeyStorage};

use anyhow::{anyhow, Result};

/// Decodes hexed string to a byte array.
fn decode_to_array<const N: usize>(s: &str) -> Result<[u8; N]> {
    let mut buf = [0; N];

    // Strip the "0x" prefix if it exists.
    let stripped = s.strip_prefix("0x").unwrap_or(s);

    // Decode
    hex::decode_to_slice(stripped, &mut buf)
        .map_err(|_| anyhow!("invalid hex format for {stripped:?}"))?;

    Ok(buf)
}
