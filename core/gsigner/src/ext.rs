// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
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

//! Extension traits for sp_core key types.
//!
//! These traits provide additional functionality on top of the standard
//! `sp_core` key types without requiring wrapper types.

use crate::error::{Result, SignerError};
use alloc::string::String;
use sp_core::crypto::Pair as PairTrait;

/// Extension trait for key pair operations.
///
/// Provides convenient methods for key generation and derivation
/// that work with any `sp_core::Pair` type.
pub trait PairExt: PairTrait + Sized {
    /// Create a key pair from a secret URI (SURI).
    ///
    /// Supports mnemonics, dev accounts (//Alice), derivation paths, and hex seeds.
    fn from_suri_ext(suri: &str, password: Option<&str>) -> Result<Self> {
        Self::from_string_with_seed(suri, password)
            .map(|(pair, _)| pair)
            .map_err(|e| SignerError::InvalidKey(alloc::format!("{e:?}")))
    }

    /// Create a key pair from a mnemonic phrase.
    fn from_phrase_ext(phrase: &str, password: Option<&str>) -> Result<Self> {
        Self::from_phrase(phrase, password)
            .map(|(pair, _)| pair)
            .map_err(|e| SignerError::InvalidKey(alloc::format!("{e:?}")))
    }

    /// Create a key pair from raw seed bytes.
    fn from_seed_bytes(seed: &[u8]) -> Result<Self> {
        Self::from_seed_slice(seed).map_err(|e| SignerError::InvalidKey(alloc::format!("{e:?}")))
    }

    /// Get the seed/secret bytes from the pair.
    ///
    /// Note: This extracts the raw secret material. Handle with care.
    fn seed_bytes(&self) -> Self::Seed {
        let raw = self.to_raw_vec();
        let mut seed = Self::Seed::default();
        let dst = seed.as_mut();
        let copy_len = core::cmp::min(dst.len(), raw.len());
        dst[..copy_len].copy_from_slice(&raw[..copy_len]);
        seed
    }
}

// Implement PairExt for all sp_core pair types
#[cfg(feature = "secp256k1")]
impl PairExt for sp_core::ecdsa::Pair {
    fn seed_bytes(&self) -> Self::Seed {
        self.seed()
    }
}

#[cfg(feature = "sr25519")]
impl PairExt for sp_core::sr25519::Pair {}

#[cfg(feature = "ed25519")]
impl PairExt for sp_core::ed25519::Pair {
    fn seed_bytes(&self) -> Self::Seed {
        self.seed()
    }
}

/// Extension trait for public key operations.
pub trait PublicExt {
    /// Convert the public key to a hex string.
    fn to_hex(&self) -> String;

    /// Get the raw bytes of the public key.
    fn as_bytes(&self) -> &[u8];
}

#[cfg(feature = "secp256k1")]
impl PublicExt for sp_core::ecdsa::Public {
    fn to_hex(&self) -> String {
        use sp_core::crypto::ByteArray;
        hex::encode(self.as_slice())
    }

    fn as_bytes(&self) -> &[u8] {
        use sp_core::crypto::ByteArray;
        self.as_slice()
    }
}

#[cfg(feature = "sr25519")]
impl PublicExt for sp_core::sr25519::Public {
    fn to_hex(&self) -> String {
        use sp_core::crypto::ByteArray;
        hex::encode(self.as_slice())
    }

    fn as_bytes(&self) -> &[u8] {
        use sp_core::crypto::ByteArray;
        self.as_slice()
    }
}

#[cfg(feature = "ed25519")]
impl PublicExt for sp_core::ed25519::Public {
    fn to_hex(&self) -> String {
        use sp_core::crypto::ByteArray;
        hex::encode(self.as_slice())
    }

    fn as_bytes(&self) -> &[u8] {
        use sp_core::crypto::ByteArray;
        self.as_slice()
    }
}

/// Extension trait for secp256k1/ECDSA-specific operations.
#[cfg(feature = "secp256k1")]
pub trait Secp256k1Ext {
    /// Convert the compressed public key to uncompressed form (64 bytes, no prefix).
    fn to_uncompressed(&self) -> [u8; 64];

    /// Derive the Ethereum address from the public key.
    fn to_ethereum_address(&self) -> crate::schemes::secp256k1::Address;
}

#[cfg(feature = "secp256k1")]
impl Secp256k1Ext for sp_core::ecdsa::Public {
    fn to_uncompressed(&self) -> [u8; 64] {
        use k256::ecdsa::VerifyingKey;

        VerifyingKey::from_sec1_bytes(self.as_ref())
            .expect("compressed key is always valid")
            .to_encoded_point(false)
            .as_bytes()[1..]
            .try_into()
            .expect("uncompressed key has 64 bytes")
    }

    fn to_ethereum_address(&self) -> crate::schemes::secp256k1::Address {
        use crate::hash::keccak256;

        let public_key_uncompressed = self.to_uncompressed();
        let hash = keccak256(&public_key_uncompressed);

        let mut address_bytes = [0u8; 20];
        address_bytes.copy_from_slice(&hash[12..]);
        crate::schemes::secp256k1::Address(address_bytes)
    }
}

/// Extension trait for Substrate address derivation.
#[cfg(any(feature = "sr25519", feature = "ed25519"))]
pub trait SubstrateAddressExt {
    /// Derive a SubstrateAddress from the public key.
    fn to_substrate_address(
        &self,
        scheme: crate::address::SubstrateCryptoScheme,
    ) -> Result<crate::address::SubstrateAddress>;
}

#[cfg(feature = "sr25519")]
impl SubstrateAddressExt for sp_core::sr25519::Public {
    fn to_substrate_address(
        &self,
        scheme: crate::address::SubstrateCryptoScheme,
    ) -> Result<crate::address::SubstrateAddress> {
        use sp_core::crypto::ByteArray;
        let bytes: [u8; 32] = self
            .as_slice()
            .try_into()
            .map_err(|_| SignerError::InvalidKey("Invalid public key length".into()))?;
        crate::address::SubstrateAddress::new(bytes, scheme)
            .map_err(|e| SignerError::InvalidAddress(e.to_string()))
    }
}

#[cfg(feature = "ed25519")]
impl SubstrateAddressExt for sp_core::ed25519::Public {
    fn to_substrate_address(
        &self,
        scheme: crate::address::SubstrateCryptoScheme,
    ) -> Result<crate::address::SubstrateAddress> {
        use sp_core::crypto::ByteArray;
        let bytes: [u8; 32] = self
            .as_slice()
            .try_into()
            .map_err(|_| SignerError::InvalidKey("Invalid public key length".into()))?;
        crate::address::SubstrateAddress::new(bytes, scheme)
            .map_err(|e| SignerError::InvalidAddress(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[cfg(feature = "secp256k1")]
    #[test]
    fn test_secp256k1_ext() {
        let (pair, _) = sp_core::ecdsa::Pair::generate();
        let public = pair.public();

        // Test to_uncompressed
        let uncompressed = public.to_uncompressed();
        assert_eq!(uncompressed.len(), 64);

        // Test to_ethereum_address
        let address = public.to_ethereum_address();
        assert_eq!(address.as_ref().len(), 20);
    }

    #[cfg(feature = "sr25519")]
    #[test]
    fn test_pair_ext_sr25519() {
        // Test from_suri_ext
        let pair = sp_core::sr25519::Pair::from_suri_ext("//Alice", None).unwrap();
        let pair2 = sp_core::sr25519::Pair::from_suri_ext("//Alice", None).unwrap();
        assert_eq!(pair.public(), pair2.public());
    }

    #[cfg(feature = "ed25519")]
    #[test]
    fn test_pair_ext_ed25519() {
        let pair = sp_core::ed25519::Pair::from_suri_ext("//Bob", None).unwrap();
        let seed = pair.seed_bytes();
        assert_eq!(seed.len(), 32);
    }
}
