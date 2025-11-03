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

//! secp256k1 ECDSA signature scheme (Ethereum-compatible).

use crate::{
    error::{Result, SignerError},
    traits::SignatureScheme,
};
use alloc::format;

pub mod address;
pub mod digest;
pub mod keys;
pub mod signature;
#[cfg(feature = "std")]
mod signer_ext;

#[cfg(all(feature = "serde", feature = "keyring"))]
pub mod keyring;

pub use address::{Address, EmptyValidatorsError, FromActorIdError, ValidatorsVec};
pub use digest::{Digest, ToDigest};
pub use keys::{PrivateKey, PublicKey};
pub use signature::{ContractSignature, Signature, SignedData, VerifiedData};

pub mod ecdsa {
    pub use super::{
        ContractSignature, PrivateKey, PublicKey, Signature, SignedData, VerifiedData,
    };
}

#[cfg(feature = "std")]
pub use crate::storage::FSKeyStorage;
pub use crate::storage::MemoryKeyStorage;
#[cfg(feature = "std")]
pub use signer_ext::Secp256k1SignerExt;

#[cfg(all(feature = "serde", feature = "keyring"))]
pub use keyring::{Keyring, Keystore as KeyringKeystore};

/// secp256k1 signature scheme marker type.
#[derive(Debug, Clone, Copy)]
pub struct Secp256k1;

impl SignatureScheme for Secp256k1 {
    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Signature = Signature;
    type Address = Address;
    type Digest = Digest;

    #[cfg(feature = "std")]
    fn generate_keypair() -> (Self::PrivateKey, Self::PublicKey) {
        let private_key = PrivateKey::random();
        let public_key = private_key.public_key();
        (private_key, public_key)
    }

    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
        private_key.public_key()
    }

    fn sign(private_key: &Self::PrivateKey, data: &[u8]) -> Result<Self::Signature> {
        Signature::create(private_key, data)
            .map_err(|e| SignerError::Crypto(format!("Signing failed: {e}")))
    }

    fn verify(
        public_key: &Self::PublicKey,
        data: &[u8],
        signature: &Self::Signature,
    ) -> Result<()> {
        signature
            .verify(*public_key, data)
            .map_err(|e| SignerError::Crypto(format!("Verification failed: {e}")))
    }

    fn address(public_key: &Self::PublicKey) -> Self::Address {
        Address::from(*public_key)
    }

    fn scheme_name() -> &'static str {
        "secp256k1"
    }
}

/// Convenient aliases for the secp256k1 signer and storages.
#[cfg(feature = "std")]
pub type Signer = crate::Signer<Secp256k1>;
pub type MemoryStorage = crate::storage::MemoryKeyStorage<Secp256k1>;
#[cfg(feature = "std")]
pub type FileStorage = crate::storage::FSKeyStorage<Secp256k1>;

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let (private_key, public_key) = Secp256k1::generate_keypair();
        let derived = Secp256k1::public_key(&private_key);
        assert_eq!(public_key, derived);
    }

    #[test]
    fn test_sign_and_verify() {
        let (private_key, public_key) = Secp256k1::generate_keypair();
        let message = b"hello world";

        let signature = Secp256k1::sign(&private_key, message).unwrap();
        Secp256k1::verify(&public_key, message, &signature).unwrap();
    }

    #[test]
    fn test_signature_recovery() {
        let (private_key, public_key) = Secp256k1::generate_keypair();
        let message = b"hello world";

        let signature = Secp256k1::sign(&private_key, message).unwrap();
        let digest = Digest::from(message.as_slice());
        let recovered = signature.validate(&digest).unwrap();

        assert_eq!(recovered, public_key);
    }
}
