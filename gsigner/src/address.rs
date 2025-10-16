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

//! Address types for different cryptographic schemes.

pub use ethexe_common::{Address, FromActorIdError};

#[cfg(feature = "sr25519")]
use anyhow::{Result, anyhow};

#[cfg(feature = "sr25519")]
use std::fmt;

/// Substrate SS58 address wrapper.
#[cfg(feature = "sr25519")]
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SubstrateAddress {
    /// The raw public key bytes (32 bytes for sr25519).
    pub public_key: [u8; 32],
    /// The SS58 encoded address string.
    ss58: String,
}

#[cfg(feature = "sr25519")]
impl SubstrateAddress {
    const DEFAULT_PREFIX: u16 = 137; // Vara network

    /// Create a new Substrate address from public key bytes.
    pub fn new(public_key: [u8; 32]) -> Result<Self> {
        use sp_core::{
            crypto::{Ss58AddressFormat, Ss58Codec},
            sr25519,
        };

        let public = sr25519::Public::from_raw(public_key);
        let format = Ss58AddressFormat::custom(Self::DEFAULT_PREFIX);
        let ss58 = public.to_ss58check_with_version(format);
        Ok(Self { public_key, ss58 })
    }

    /// Get the SS58 encoded address string.
    pub fn as_ss58(&self) -> &str {
        &self.ss58
    }

    /// Get the raw public key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.public_key
    }

    /// Decode an SS58 address string to public key bytes.
    pub fn from_ss58(ss58: &str) -> Result<Self> {
        use sp_core::{crypto::Ss58Codec, sr25519};

        let public = sr25519::Public::from_ss58check(ss58)
            .map_err(|e| anyhow!("Invalid SS58 encoding: {e}"))?;
        let public_key = public.0;
        Ok(Self {
            public_key,
            ss58: ss58.to_string(),
        })
    }

    /// Re-encode address to a different SS58 format (e.g., VARA).
    pub fn recode(&self) -> Result<Self> {
        Self::new(self.public_key)
    }
}

#[cfg(feature = "sr25519")]
impl fmt::Debug for SubstrateAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubstrateAddress")
            .field("ss58", &self.ss58)
            .finish()
    }
}

#[cfg(feature = "sr25519")]
impl fmt::Display for SubstrateAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ss58)
    }
}

#[cfg(feature = "sr25519")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substrate_address_encode_decode() {
        let public_key = [0x42; 32];
        let addr = SubstrateAddress::new(public_key).unwrap();
        let ss58 = addr.as_ss58();

        let decoded = SubstrateAddress::from_ss58(ss58).unwrap();
        assert_eq!(decoded.public_key, public_key);
    }
}
