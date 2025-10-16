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

//! Substrate integration for gsigner.
//!
//! This module provides compatibility with Substrate/Polkadot ecosystem,
//! including integration with subxt for transaction signing.

#[cfg(feature = "keyring")]
use crate::keyring::KeystoreEntry;
use crate::{
    schemes::sr25519::{PrivateKey, PublicKey, Signature},
    traits::SignatureScheme,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Substrate pair-like interface for sr25519 keys.
///
/// This provides a Substrate-compatible API for key management and signing,
/// compatible with `sp_core::Pair` interface patterns.
#[derive(Clone, Serialize, Deserialize)]
pub struct SubstratePair {
    name: String,
    pub address: String,
    private_key: PrivateKey,
    public_key: PublicKey,
}

impl SubstratePair {
    /// Create a new pair from a private key.
    pub fn from_private_key(name: &str, private_key: PrivateKey) -> Self {
        let public_key = crate::schemes::sr25519::Sr25519::public_key(&private_key);
        let address = format!("0x{}", hex::encode(public_key.to_bytes()));
        Self {
            name: name.to_string(),
            address,
            private_key,
            public_key,
        }
    }

    /// Create from SURI (Secret URI) format.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let pair = SubstratePair::from_suri("alice", "//Alice", None)?;
    /// let pair = SubstratePair::from_suri(
    ///     "stash",
    ///     "bottom drive obey lake curtain smoke basket hold race lonely fit walk//Alice",
    ///     None,
    /// )?;
    /// ```
    pub fn from_suri(name: &str, suri: &str, password: Option<&str>) -> Result<Self> {
        let private_key = PrivateKey::from_suri(suri, password)?;
        Ok(Self::from_private_key(name, private_key))
    }

    /// Generate a new random pair.
    pub fn generate(name: &str) -> Self {
        let private_key = PrivateKey::random();
        Self::from_private_key(name, private_key)
    }

    /// Get the public key.
    pub fn public(&self) -> &PublicKey {
        &self.public_key
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> Signature {
        crate::schemes::sr25519::Sr25519::sign(&self.private_key, message)
            .expect("stored sr25519 key is always valid")
    }

    /// Get the private key bytes (half-ed25519 format).
    pub fn to_raw_vec(&self) -> Vec<u8> {
        self.private_key.to_bytes().to_vec()
    }

    /// Get public key bytes.
    pub fn public_bytes(&self) -> [u8; 32] {
        self.public_key.to_bytes()
    }

    /// Decrypt method for compatibility (no-op as keys are already in memory).
    /// Returns a clone of self.
    pub fn decrypt(&self, _password: Option<&[u8]>) -> Result<Self> {
        Ok(self.clone())
    }
}

/// Convert gsigner types to sp_core types (when sp-core is available).
#[cfg(feature = "sp-core")]
pub mod sp_compat {
    use super::*;
    use sp_core::{crypto::Ss58Codec, sr25519};
    use sp_runtime::AccountId32;

    impl SubstratePair {
        /// Convert to sp_core::sr25519::Pair.
        pub fn to_sp_pair(&self) -> Result<sr25519::Pair> {
            let keypair = self
                .private_key
                .keypair()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            Ok(sr25519::Pair::from(keypair))
        }

        /// Create from sp_core::sr25519::Pair.
        pub fn from_sp_pair(name: &str, pair: &sr25519::Pair) -> Result<Self> {
            let keypair: schnorrkel::Keypair = pair.clone().into();
            Ok(Self::from_private_key(
                name,
                PrivateKey::from_keypair(keypair),
            ))
        }

        /// Get AccountId32 from public key.
        pub fn account_id(&self) -> AccountId32 {
            AccountId32::from(self.public_bytes())
        }

        /// Get SS58 address with default format.
        pub fn to_ss58check(&self) -> String {
            self.account_id().to_ss58check()
        }

        /// Get SS58 address with custom format.
        pub fn to_ss58check_with_version(
            &self,
            version: sp_core::crypto::Ss58AddressFormat,
        ) -> String {
            self.account_id().to_ss58check_with_version(version)
        }
    }

    impl From<SubstratePair> for sr25519::Pair {
        fn from(pair: SubstratePair) -> Self {
            pair.to_sp_pair()
                .expect("Failed to convert to sp_core pair")
        }
    }

    impl From<sr25519::Pair> for SubstratePair {
        fn from(pair: sr25519::Pair) -> Self {
            SubstratePair::from_sp_pair("default", &pair)
                .expect("Failed to convert from sp_core pair")
        }
    }
}

#[cfg(feature = "keyring")]
impl KeystoreEntry for SubstratePair {
    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substrate_pair_generation() {
        let pair = SubstratePair::generate("test");
        let message = b"test message";
        let signature = pair.sign(message);

        // Verify using schnorrkel directly
        use schnorrkel::signing_context;
        const SIGNING_CONTEXT: &[u8] = b"gsigner";
        let context = signing_context(SIGNING_CONTEXT);
        let public_key = schnorrkel::PublicKey::from_bytes(&pair.public_bytes()).unwrap();
        let sig = schnorrkel::Signature::from_bytes(&signature.to_bytes()).unwrap();
        assert!(public_key.verify(context.bytes(message), &sig).is_ok());
    }

    #[test]
    fn test_substrate_pair_from_suri() {
        let pair = SubstratePair::from_suri("alice", "//Alice", None).unwrap();
        assert_eq!(pair.public_bytes().len(), 32);
    }

    #[cfg(all(feature = "sp-core", target_arch = "wasm32"))]
    #[test]
    fn test_sp_core_conversion() {
        use sp_compat::*;

        let pair = SubstratePair::generate("test");
        let sp_pair = pair.to_sp_pair().unwrap();

        // Convert back
        let converted = SubstratePair::from_sp_pair("converted", &sp_pair).unwrap();
        assert_eq!(pair.public_bytes(), converted.public_bytes());
    }

    #[cfg(all(feature = "sp-core", target_arch = "wasm32"))]
    #[test]
    fn test_account_id() {
        use sp_compat::*;

        let pair = SubstratePair::from_suri("alice", "//Alice", None).unwrap();
        let account_id = pair.account_id();
        let ss58 = pair.to_ss58check();

        assert!(!ss58.is_empty());
        assert_eq!(account_id.as_ref().len(), 32);
    }
}
