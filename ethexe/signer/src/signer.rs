// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use crate::storage::{FSKeyStorage, KeyStorage};
use anyhow::Result;
use ethexe_common::{
    k256::{ecdsa::SigningKey, elliptic_curve::rand_core},
    Address, ContractSignature, Digest, PrivateKey, PublicKey, Signature, SignedData,
};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

/// Signer which signs data using owned key store.
#[derive(Debug, Clone)]
pub struct Signer {
    key_store: Arc<RwLock<dyn KeyStorage>>,
}

impl Signer {
    /// Create a new signer with a key store.
    pub fn new(key_store: impl KeyStorage) -> Self {
        Self {
            key_store: Arc::new(RwLock::new(key_store)),
        }
    }

    /// Create a new signer with an empty key store.
    pub fn empty<S: KeyStorage + Sized>() -> Self {
        Self::new(S::empty())
    }

    /// Create a new signer with a key store location.
    pub fn fs(path: PathBuf) -> Self {
        fs::create_dir_all(path.as_path()).expect("Cannot create key store dir");

        Self::new(FSKeyStorage::from_path(path))
    }

    /// Create a new signer with keys from the provided sub-set `keys`.
    pub fn sub_signer<S: KeyStorage + Sized>(&self, keys: Vec<PublicKey>) -> Result<Self> {
        let mut key_store = S::empty();
        for key in keys {
            let private_key = self.storage().get_private_key(key)?;
            key_store.add_key(private_key)?;
        }

        Ok(Self::new(key_store))
    }

    /// Create a ECDSA recoverable signature.
    pub fn sign<T>(&self, public_key: PublicKey, data: T) -> Result<Signature>
    where
        Digest: From<T>,
    {
        let private_key = self.storage().get_private_key(public_key)?;

        Signature::create(private_key, data).map_err(Into::into)
    }

    /// Create a ECDSA recoverable signature packed with data together.
    pub fn signed_data<T: Sized>(&self, public_key: PublicKey, data: T) -> Result<SignedData<T>>
    where
        for<'a> Digest: From<&'a T>,
    {
        SignedData::create(self.storage().get_private_key(public_key)?, data).map_err(Into::into)
    }

    /// Create a ECDSA recoverable contract-specific signature.
    pub fn sign_for_contract<T>(
        &self,
        contract_address: Address,
        public_key: PublicKey,
        data: T,
    ) -> Result<ContractSignature>
    where
        Digest: From<T>,
    {
        let private_key = self.storage().get_private_key(public_key)?;

        ContractSignature::create(contract_address, private_key, data).map_err(Into::into)
    }

    /// Generate a new private key and return a public key for it.
    pub fn generate_key(&self) -> Result<PublicKey> {
        let private_key: PrivateKey = SigningKey::random(&mut rand_core::OsRng).into();

        let public_key = self.storage_mut().add_key(private_key)?;

        log::debug!(
            "New private key generated and stored
            PrivateKey: {private_key}
            PublicKey: {public_key}"
        );

        Ok(public_key)
    }

    /// Get a key storage for immutable access.
    pub fn storage(&self) -> RwLockReadGuard<'_, dyn KeyStorage> {
        self.key_store.read().expect("Failed to access key store")
    }

    /// Get a key storage for mutable access.
    pub fn storage_mut(&self) -> RwLockWriteGuard<'_, dyn KeyStorage> {
        self.key_store.write().expect("Failed to access key store")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryKeyStorage;
    use alloy::primitives::{keccak256, PrimitiveSignature};
    use std::str::FromStr;

    #[test]
    fn test_signer_with_known_vectors() {
        let signer = Signer::empty::<MemoryKeyStorage>();

        let private_key_hex = "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f";

        let message = b"hello world";

        // Convert the private key hex to bytes and add it to the signer
        let private_key = PrivateKey::from_str(private_key_hex).expect("Invalid private key hex");
        let public_key = signer
            .storage_mut()
            .add_key(private_key)
            .expect("Failed to add key");

        // Ensure the key store has the key
        assert!(signer.storage().has_key(public_key).unwrap());

        // Sign the message
        let signature = signer
            .sign(public_key, message.as_slice())
            .expect("Failed to sign message");

        // Hash the message using Keccak256
        let hash = keccak256(message);

        // Recover the address using the signature
        let alloy_sig = PrimitiveSignature::try_from(signature.into_pre_eip155_bytes().as_ref())
            .expect("failed to parse sig");

        let recovered_address = alloy_sig
            .recover_address_from_prehash(&hash)
            .expect("Failed to recover address");

        // Verify the recovered address matches the expected address
        assert_eq!(
            format!("{:?}", recovered_address),
            format!("{}", public_key.to_address())
        );
    }

    #[test]
    fn recover_digest() {
        let signer = Signer::empty::<MemoryKeyStorage>();

        let private_key_hex = "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f";
        let message = b"hello world";

        let private_key = PrivateKey::from_str(private_key_hex).expect("Invalid private key hex");
        let public_key = signer
            .storage_mut()
            .add_key(private_key)
            .expect("Failed to add key");

        let signature = signer
            .sign(public_key, message.as_slice())
            .expect("Failed to sign message");

        let hash = keccak256(message).0;

        let recovered_public_key = signature
            .recover(Digest::from(hash))
            .expect("Failed to recover public key");

        assert_eq!(recovered_public_key, public_key);
    }

    #[test]
    fn signed_data() {
        let signer = Signer::empty::<MemoryKeyStorage>();

        let public_key = signer.generate_key().unwrap();

        let signed_data = signer
            .signed_data(public_key, b"hello world".as_slice())
            .expect("Failed to create signed data");

        assert_eq!(signed_data.data(), b"hello world");
        assert_eq!(signed_data.public_key(), public_key);
    }
}
