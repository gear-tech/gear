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

//! secp256k1 ECDSA signature scheme (Ethereum-compatible).

use crate::{
    error::{Result, SignerError},
    scheme::CryptoScheme,
};
use alloc::{format, string::String, vec::Vec};

pub mod address;
pub mod digest;
pub mod keys;
pub mod signature;
#[cfg(feature = "std")]
mod signer_ext;

#[cfg(all(feature = "serde", feature = "keyring"))]
pub mod keyring;
#[cfg(all(feature = "serde", feature = "keyring"))]
pub use keyring::{Keyring, Keystore, Secp256k1Codec};

pub use address::{Address, FromActorIdError};
pub use digest::{Digest, ToDigest};
pub use keys::{PrivateKey, PublicKey, Seed};
pub use signature::{ContractSignature, Signature, SignedData, SignedMessage, VerifiedData};

pub mod ecdsa {
    pub use super::{
        ContractSignature, PrivateKey, PublicKey, Signature, SignedData, SignedMessage,
        VerifiedData,
    };
}

#[cfg(feature = "std")]
pub use signer_ext::Secp256k1SignerExt;

/// secp256k1 signature scheme marker type.
#[derive(Debug, Clone, Copy)]
pub struct Secp256k1;

impl CryptoScheme for Secp256k1 {
    const NAME: &'static str = "secp256k1";
    const NAMESPACE: &'static str = "secp";
    const PUBLIC_KEY_SIZE: usize = 33;
    const SIGNATURE_SIZE: usize = 65;

    type PrivateKey = PrivateKey;
    type PublicKey = PublicKey;
    type Signature = Signature;
    type Address = Address;
    type Seed = Seed;

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

    fn to_address(public_key: &Self::PublicKey) -> Self::Address {
        Address::from(*public_key)
    }

    fn public_key_to_bytes(public_key: &Self::PublicKey) -> Vec<u8> {
        public_key.to_bytes().to_vec()
    }

    fn public_key_from_bytes(bytes: &[u8]) -> Result<Self::PublicKey> {
        if bytes.len() != Self::PUBLIC_KEY_SIZE {
            return Err(SignerError::InvalidKey(format!(
                "Expected {} bytes, got {}",
                Self::PUBLIC_KEY_SIZE,
                bytes.len()
            )));
        }
        let mut arr = [0u8; 33];
        arr.copy_from_slice(bytes);
        PublicKey::from_bytes(arr)
    }

    fn signature_to_bytes(signature: &Self::Signature) -> Vec<u8> {
        signature.into_pre_eip155_bytes().to_vec()
    }

    fn signature_from_bytes(bytes: &[u8]) -> Result<Self::Signature> {
        if bytes.len() != Self::SIGNATURE_SIZE {
            return Err(SignerError::InvalidSignature(format!(
                "Expected {} bytes, got {}",
                Self::SIGNATURE_SIZE,
                bytes.len()
            )));
        }
        let mut arr = [0u8; 65];
        arr.copy_from_slice(bytes);
        Signature::from_pre_eip155_bytes(arr)
            .ok_or_else(|| SignerError::InvalidSignature("Invalid signature bytes".into()))
    }

    fn address_to_string(address: &Self::Address) -> String {
        format!("0x{}", address.to_hex())
    }

    fn private_key_from_seed(seed: Self::Seed) -> Result<Self::PrivateKey> {
        Ok(PrivateKey::from_pair_seed(seed))
    }

    fn private_key_to_seed(private_key: &Self::PrivateKey) -> Self::Seed {
        private_key.seed()
    }

    #[cfg(feature = "std")]
    fn private_key_from_suri(suri: &str, password: Option<&str>) -> Result<Self::PrivateKey> {
        PrivateKey::from_suri(suri, password)
    }
}

/// Convenient alias for the secp256k1 signer.
#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
pub type Signer = crate::Signer<Secp256k1>;

#[cfg(all(feature = "std", feature = "keyring", feature = "serde"))]
impl crate::keyring::KeyringScheme for Secp256k1 {
    type Keystore = keyring::Keystore;

    fn namespace() -> &'static str {
        crate::keyring::NAMESPACE_SECP
    }

    fn keystore_from_private(
        name: &str,
        private_key: &Self::PrivateKey,
        password: Option<&str>,
    ) -> crate::error::Result<Self::Keystore> {
        Ok(Self::Keystore::from_private_key_with_password(
            name,
            private_key.clone(),
            password,
        )?)
    }

    fn keystore_private(
        keystore: &Self::Keystore,
        password: Option<&str>,
    ) -> crate::error::Result<Self::PrivateKey> {
        Ok(keystore.private_key_with_password(password)?)
    }

    fn keystore_public(keystore: &Self::Keystore) -> crate::error::Result<Self::PublicKey> {
        Ok(keystore.public_key()?)
    }

    fn keystore_address(keystore: &Self::Keystore) -> crate::error::Result<Self::Address> {
        Ok(keystore.address()?)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::scheme::CryptoScheme;

    #[test]
    fn test_keypair_generation() {
        let (private_key, public_key) = <Secp256k1 as CryptoScheme>::generate_keypair();
        let derived = <Secp256k1 as CryptoScheme>::public_key(&private_key);
        assert_eq!(public_key, derived);
    }

    #[test]
    fn test_sign_and_verify() {
        let (private_key, public_key) = <Secp256k1 as CryptoScheme>::generate_keypair();
        let message = b"hello world";

        let signature = <Secp256k1 as CryptoScheme>::sign(&private_key, message).unwrap();
        <Secp256k1 as CryptoScheme>::verify(&public_key, message, &signature).unwrap();
    }

    #[test]
    fn test_signature_recovery() {
        let (private_key, public_key) = <Secp256k1 as CryptoScheme>::generate_keypair();
        let message = b"hello world";

        let signature = <Secp256k1 as CryptoScheme>::sign(&private_key, message).unwrap();
        let digest = Digest::from(message.as_slice());
        let recovered = signature.validate(&digest).unwrap();

        assert_eq!(recovered, public_key);
    }
}
