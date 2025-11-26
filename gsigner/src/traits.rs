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

//! Core traits for signature schemes and key storage.

use crate::error::Result;
use alloc::vec::Vec;
use core::{fmt::Debug, hash::Hash};

/// Trait defining a cryptographic signature scheme.
///
/// Implementors of this trait provide concrete implementations for different
/// cryptographic algorithms (e.g., secp256k1, sr25519).
pub trait SignatureScheme: Debug + Send + Sync + 'static {
    /// Human-readable name of the scheme.
    const NAME: &'static str;

    /// The private key type for this scheme.
    type PrivateKey: Clone + Debug + Send + Sync;

    /// The public key type for this scheme.
    type PublicKey: Clone + Debug + Send + Sync + Eq + Hash + Ord;

    /// The signature type for this scheme.
    type Signature: Clone + Debug + Send + Sync;

    /// The address type for this scheme.
    type Address: Clone + Debug + Send + Sync + PartialEq;

    /// The digest/hash type used for signing.
    type Digest: AsRef<[u8]>;

    /// Generate a new random keypair.
    #[cfg(feature = "std")]
    fn generate_keypair() -> (Self::PrivateKey, Self::PublicKey);

    /// Derive public key from private key.
    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey;

    /// Return a stable byte representation of the public key.
    fn public_key_bytes(public_key: &Self::PublicKey) -> Vec<u8>;

    /// Sign data with a private key.
    fn sign(private_key: &Self::PrivateKey, data: &[u8]) -> Result<Self::Signature>;

    /// Verify a signature with a public key.
    fn verify(public_key: &Self::PublicKey, data: &[u8], signature: &Self::Signature)
    -> Result<()>;

    /// Derive address from public key.
    fn address(public_key: &Self::PublicKey) -> Self::Address;

    /// Get the scheme name for display purposes.
    fn scheme_name() -> &'static str {
        Self::NAME
    }
}

/// Trait implemented by private key types that can be reconstructed from their seed.
pub trait SeedableKey: Clone {
    /// Seed type associated with the private key.
    type Seed: Clone + Default + AsRef<[u8]> + AsMut<[u8]> + Send + Sync + 'static;

    /// Reconstruct the private key from its seed.
    fn from_seed(seed: Self::Seed) -> Result<Self>
    where
        Self: Sized;

    /// Export the private key seed.
    fn seed(&self) -> Self::Seed;
}
