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

//! Signer library for ethexe.

mod digest;
mod signature;

pub use digest::{Digest, ToDigest};
use secp256k1::hashes::hex::{Case, DisplayHex};
pub use sha3;
pub use signature::Signature;

use anyhow::{anyhow, bail, Result};
use gprimitives::{ActorId, H160};
use parity_scale_codec::{Decode, Encode};
use sha3::Digest as _;
use signature::RawSignature;
use std::{fmt, fs, path::PathBuf, str::FromStr};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PublicKey(pub [u8; 33]);

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrivateKey(pub [u8; 32]);

impl From<PrivateKey> for PublicKey {
    fn from(key: PrivateKey) -> Self {
        let secret_key =
            secp256k1::SecretKey::from_slice(&key.0[..]).expect("32 bytes, within curve order");
        let public_key = secp256k1::PublicKey::from_secret_key_global(&secret_key);

        PublicKey::from_bytes(public_key.serialize())
    }
}

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address(pub [u8; 20]);

impl From<[u8; 20]> for Address {
    fn from(value: [u8; 20]) -> Self {
        Self(value)
    }
}

impl From<H160> for Address {
    fn from(value: H160) -> Self {
        Self(value.into())
    }
}

impl TryFrom<ActorId> for Address {
    type Error = anyhow::Error;

    fn try_from(id: ActorId) -> std::result::Result<Self, Self::Error> {
        id.as_ref()
            .iter()
            .take(12)
            .all(|&byte| byte == 0)
            .then_some(Address(id.to_address_lossy().0))
            .ok_or_else(|| anyhow!("First 12 bytes are not 0, it is not ethereum address"))
    }
}

impl From<Address> for ActorId {
    fn from(value: Address) -> Self {
        H160(value.0).into()
    }
}

fn strip_prefix(s: &str) -> &str {
    if let Some(s) = s.strip_prefix("0x") {
        s
    } else {
        s
    }
}

fn decode_to_array<const N: usize>(s: &str) -> Result<[u8; N]> {
    let mut buf = [0; N];
    hex::decode_to_slice(strip_prefix(s), &mut buf)
        .map_err(|_| anyhow!("invalid hex format for {s:?}"))?;
    Ok(buf)
}

impl FromStr for PrivateKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(decode_to_array(s)?))
    }
}

impl PublicKey {
    pub fn from_bytes(bytes: [u8; 33]) -> Self {
        Self(bytes)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn to_address(&self) -> Address {
        let public_key_uncompressed = secp256k1::PublicKey::from_slice(&self.0)
            .expect("Invalid public key")
            .serialize_uncompressed();

        let mut address = Address::default();
        let hash = sha3::Keccak256::digest(&public_key_uncompressed[1..]);
        address.0[..20].copy_from_slice(&hash[12..]);

        address
    }
}

impl FromStr for PublicKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(decode_to_array(s)?))
    }
}

impl Address {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl FromStr for Address {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(decode_to_array(s)?))
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

#[derive(Debug, Clone)]
pub struct Signer {
    key_store: PathBuf,
}

impl Signer {
    pub fn new(key_store: PathBuf) -> Result<Self> {
        fs::create_dir_all(key_store.as_path())?;

        Ok(Self { key_store })
    }

    pub fn tmp() -> Self {
        let temp_dir = tempfile::tempdir().expect("Cannot create temp dir for keys");
        Self {
            key_store: temp_dir.into_path(),
        }
    }

    pub fn raw_sign_digest(&self, public_key: PublicKey, digest: Digest) -> Result<RawSignature> {
        let private_key = self.get_private_key(public_key)?;

        RawSignature::create_for_digest(private_key, digest)
    }

    pub fn sign_digest(&self, public_key: PublicKey, digest: Digest) -> Result<Signature> {
        let private_key = self.get_private_key(public_key)?;

        Signature::create_for_digest(private_key, digest)
    }

    pub fn sign(&self, public_key: PublicKey, data: &[u8]) -> Result<Signature> {
        self.sign_digest(public_key, data.to_digest())
    }

    pub fn sign_with_addr(&self, address: Address, data: &[u8]) -> Result<Signature> {
        let keys = self.list_keys()?;

        for key in keys {
            if key.to_address() == address {
                return self.sign(key, data);
            }
        }

        bail!("Address not found: {}", address);
    }

    pub fn get_key_by_addr(&self, address: Address) -> Result<Option<PublicKey>> {
        let keys = self.list_keys()?;

        for key in keys {
            if key.to_address() == address {
                return Ok(Some(key));
            }
        }

        Ok(None)
    }

    pub fn has_addr(&self, address: Address) -> Result<bool> {
        Ok(self.get_key_by_addr(address)?.is_some())
    }

    pub fn has_key(&self, key: PublicKey) -> Result<bool> {
        let key_path = self.key_store.join(key.to_hex());
        let has_key = fs::metadata(key_path).is_ok();
        Ok(has_key)
    }

    pub fn add_key(&self, key: PrivateKey) -> Result<PublicKey> {
        let secret_key =
            secp256k1::SecretKey::from_slice(&key.0[..]).expect("32 bytes, within curve order");
        let public_key = secp256k1::PublicKey::from_secret_key_global(&secret_key);

        let local_public = PublicKey::from_bytes(public_key.serialize());

        let key_file = self.key_store.join(local_public.to_hex());
        fs::write(key_file, secret_key.secret_bytes())?;
        Ok(local_public)
    }

    pub fn generate_key(&self) -> Result<PublicKey> {
        let (secret_key, public_key) =
            secp256k1::generate_keypair(&mut secp256k1::rand::thread_rng());

        let local_public = PublicKey::from_bytes(public_key.serialize());

        log::debug!(
            "Secret key generated: {}",
            secret_key.secret_bytes().to_hex_string(Case::Lower)
        );

        let key_file = self.key_store.join(local_public.to_hex());
        fs::write(key_file, secret_key.secret_bytes())?;
        Ok(local_public)
    }

    pub fn clear_keys(&self) -> Result<()> {
        fs::remove_dir_all(&self.key_store)?;

        Ok(())
    }

    pub fn list_keys(&self) -> Result<Vec<PublicKey>> {
        let mut keys = vec![];

        for entry in fs::read_dir(&self.key_store)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let key = PublicKey::from_str(file_name.to_string_lossy().as_ref())?;
            keys.push(key);
        }

        Ok(keys)
    }

    pub fn get_private_key(&self, key: PublicKey) -> Result<PrivateKey> {
        let mut buf = [0u8; 32];

        let key_path = self.key_store.join(key.to_hex());
        let bytes = fs::read(key_path)?;

        if bytes.len() != 32 {
            bail!("Invalid key length: {:?}", bytes);
        }

        buf.copy_from_slice(&bytes);

        Ok(PrivateKey(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{keccak256, PrimitiveSignature as Signature};
    use std::env::temp_dir;

    #[test]
    fn test_signer_with_known_vectors() {
        // Known test vector data
        let private_key_hex = "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f";

        let message = b"hello world";

        // Create the signer with a temporary key store path
        let key_store = PathBuf::from("/tmp/key-store-test-vectors");
        let signer = Signer::new(key_store.clone()).expect("Failed to create signer");

        // Convert the private key hex to bytes and add it to the signer
        let private_key = PrivateKey::from_str(private_key_hex).expect("Invalid private key hex");
        let public_key = signer.add_key(private_key).expect("Failed to add key");

        // Ensure the key store has the key
        assert!(signer.has_key(public_key).unwrap());

        // Sign the message
        let signature = signer
            .sign(public_key, message)
            .expect("Failed to sign message");

        // Hash the message using Keccak256
        let hash = keccak256(message);

        // Recover the address using the signature
        let alloy_sig = Signature::try_from(signature.as_ref()).expect("failed to parse sig");

        let recovered_address = alloy_sig
            .recover_address_from_prehash(&hash)
            .expect("Failed to recover address");

        // Verify the recovered address matches the expected address
        assert_eq!(
            format!("{:?}", recovered_address),
            format!("{}", public_key.to_address())
        );

        // Clean up the key store directory
        signer.clear_keys().unwrap();
    }

    #[test]
    fn test_signer_with_addr() {
        // Create the signer with a temporary key store path
        let key_store = PathBuf::from("/tmp/key-store-test-addr");
        let signer = Signer::new(key_store.clone()).expect("Failed to create signer");

        // Generate a new key
        let public_key = signer.generate_key().expect("Failed to generate key");

        // Ensure the key store has the key
        assert!(signer.has_key(public_key).unwrap());

        // Sign the message
        let message = b"hello world";
        let signature = signer
            .sign_with_addr(public_key.to_address(), message)
            .expect("Failed to sign message");

        // Hash the message using Keccak256
        let hash = keccak256(message);

        // Recover the address using the signature
        let alloy_sig = Signature::try_from(signature.as_ref()).expect("failed to parse sig");

        let recovered_address = alloy_sig
            .recover_address_from_prehash(&hash)
            .expect("Failed to recover address");

        // Verify the recovered address matches the expected address
        assert_eq!(
            format!("{:?}", recovered_address),
            format!("{}", public_key.to_address())
        );

        // Clean up the key store directory
        signer.clear_keys().unwrap();
    }

    #[test]
    fn try_from_actor_id() {
        let id =
            ActorId::from_str("0x0000000000000000000000006e4c403878dbcb0dadcbe562346e8387f9542829")
                .unwrap();
        Address::try_from(id).expect("Must be correct ethereum address");

        let id =
            ActorId::from_str("0x1111111111111111111111116e4c403878dbcb0dadcbe562346e8387f9542829")
                .unwrap();
        Address::try_from(id).expect_err("Must be incorrect ethereum address");
    }

    #[test]
    fn recover_digest() {
        let private_key_hex = "4c0883a69102937d6231471b5dbb6204fe51296170827936ea5cce4b76994b0f";
        let message = b"hello world";

        let key_store = temp_dir().join("signer-tests");
        let signer = Signer::new(key_store).expect("Failed to create signer");

        let private_key = PrivateKey::from_str(private_key_hex).expect("Invalid private key hex");
        let public_key = signer.add_key(private_key).expect("Failed to add key");

        let signature = signer
            .sign(public_key, message)
            .expect("Failed to sign message");

        let hash = keccak256(message).0;

        let recovered_public_key = signature
            .recover_from_digest(hash.into())
            .expect("Failed to recover public key");

        assert_eq!(recovered_public_key, public_key);
    }
}
