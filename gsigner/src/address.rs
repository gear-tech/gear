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

//! Address types for different cryptographic schemes.

#[cfg(feature = "secp256k1")]
pub use crate::schemes::secp256k1::{Address, FromActorIdError};

#[cfg(any(feature = "sr25519", feature = "ed25519"))]
use crate::error::{Result, SignerError};
#[cfg(any(feature = "sr25519", feature = "ed25519"))]
use alloc::{
    format,
    string::{String, ToString},
};
#[cfg(any(feature = "sr25519", feature = "ed25519"))]
use derive_more::{Debug, Display};
#[cfg(any(feature = "sr25519", feature = "ed25519"))]
use sp_core::crypto::Ss58AddressFormat;
#[cfg(all(feature = "serde", any(feature = "sr25519", feature = "ed25519")))]
use sp_core::crypto::Ss58Codec;

/// Substrate SS58 address wrapper.
#[cfg(any(feature = "sr25519", feature = "ed25519"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubstrateCryptoScheme {
    #[cfg(feature = "sr25519")]
    Sr25519,
    #[cfg(feature = "ed25519")]
    Ed25519,
}

#[cfg(any(feature = "sr25519", feature = "ed25519"))]
#[derive(Clone, PartialEq, Eq, Hash, Display, Debug)]
#[display("{ss58}")]
pub struct SubstrateAddress {
    /// The raw public key bytes (32 bytes for Substrate-compatible schemes).
    pub public_key: [u8; 32],
    /// The SS58 encoded address string.
    ss58: String,
    /// The cryptographic scheme used to derive this address.
    scheme: SubstrateCryptoScheme,
}

#[cfg(any(feature = "sr25519", feature = "ed25519"))]
impl SubstrateAddress {
    pub(crate) const DEFAULT_PREFIX: u16 = 137; // Vara network

    /// Create a new Substrate address from public key bytes for the given scheme.
    pub fn new(public_key: [u8; 32], scheme: SubstrateCryptoScheme) -> Result<Self> {
        Self::new_with_format(
            public_key,
            scheme,
            Ss58AddressFormat::custom(Self::DEFAULT_PREFIX),
        )
    }

    /// Create a new Substrate address with a custom SS58 format.
    pub fn new_with_format(
        public_key: [u8; 32],
        scheme: SubstrateCryptoScheme,
        format: Ss58AddressFormat,
    ) -> Result<Self> {
        let ss58 = Self::encode(public_key, format, scheme)?;
        Ok(Self {
            public_key,
            ss58,
            scheme,
        })
    }

    #[cfg(feature = "sr25519")]
    /// Convenience constructor for sr25519 public keys.
    pub fn from_sr25519(public_key: [u8; 32]) -> Result<Self> {
        Self::new(public_key, SubstrateCryptoScheme::Sr25519)
    }

    #[cfg(feature = "ed25519")]
    /// Convenience constructor for ed25519 public keys.
    pub fn from_ed25519(public_key: [u8; 32]) -> Result<Self> {
        Self::new(public_key, SubstrateCryptoScheme::Ed25519)
    }

    /// Get the SS58 encoded address string.
    pub fn as_ss58(&self) -> &str {
        &self.ss58
    }

    /// Get the raw public key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.public_key
    }

    /// Get the underlying cryptographic scheme.
    pub fn scheme(&self) -> SubstrateCryptoScheme {
        self.scheme
    }

    /// Decode an SS58 address string to public key bytes.
    #[cfg(feature = "serde")]
    pub fn from_ss58(ss58: &str) -> Result<Self> {
        #[cfg(feature = "sr25519")]
        if let Ok(public) = sp_core::sr25519::Public::from_ss58check(ss58) {
            return Ok(Self {
                public_key: public.0,
                ss58: ss58.to_string(),
                scheme: SubstrateCryptoScheme::Sr25519,
            });
        }

        #[cfg(feature = "ed25519")]
        if let Ok(public) = sp_core::ed25519::Public::from_ss58check(ss58) {
            return Ok(Self {
                public_key: public.0,
                ss58: ss58.to_string(),
                scheme: SubstrateCryptoScheme::Ed25519,
            });
        }

        Err(SignerError::InvalidAddress(format!(
            "Invalid SS58 encoding: {ss58}"
        )))
    }

    /// Re-encode address to a different SS58 format (e.g., VARA).
    pub fn recode(&self) -> Result<Self> {
        Self::new(self.public_key, self.scheme)
    }

    fn encode(
        public_key: [u8; 32],
        format: Ss58AddressFormat,
        scheme: SubstrateCryptoScheme,
    ) -> Result<String> {
        match scheme {
            #[cfg(feature = "sr25519")]
            SubstrateCryptoScheme::Sr25519 => {
                let public = sp_core::sr25519::Public::from_raw(public_key);
                Ok(public.to_ss58check_with_version(format))
            }
            #[cfg(feature = "ed25519")]
            SubstrateCryptoScheme::Ed25519 => {
                let public = sp_core::ed25519::Public::from_raw(public_key);
                Ok(public.to_ss58check_with_version(format))
            }
            #[cfg(not(any(feature = "sr25519", feature = "ed25519")))]
            _ => Err(SignerError::FeatureNotEnabled("substrate address encoding")),
        }
    }
}

#[cfg(any(feature = "sr25519", feature = "ed25519"))]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substrate_address_encode_decode() {
        let public_key = [0x42; 32];
        #[cfg(feature = "sr25519")]
        let scheme = SubstrateCryptoScheme::Sr25519;
        #[cfg(all(not(feature = "sr25519"), feature = "ed25519"))]
        let scheme = SubstrateCryptoScheme::Ed25519;

        let addr = SubstrateAddress::new(public_key, scheme).unwrap();
        let ss58 = addr.as_ss58();

        let decoded = SubstrateAddress::from_ss58(ss58).unwrap();
        assert_eq!(decoded.public_key, public_key);
        assert_eq!(decoded.scheme(), scheme);
    }
}
