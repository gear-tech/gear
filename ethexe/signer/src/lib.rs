// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use anyhow::{anyhow, Context as _, Result};
use gprimitives::ActorId;
use parity_scale_codec::{Decode, Encode};
use secp256k1::Message;
use sha3::Digest as _;
use std::{fmt, fs, path::PathBuf, str::FromStr};

#[derive(Debug, Clone, Copy)]
pub struct PublicKey(pub [u8; 33]);

pub struct PrivateKey(pub [u8; 32]);

#[derive(Encode, Decode, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Address(pub [u8; 20]);

impl TryFrom<ActorId> for Address {
    type Error = anyhow::Error;

    fn try_from(id: ActorId) -> std::result::Result<Self, Self::Error> {
        id.as_ref()
            .iter()
            .take(12)
            .all(|&byte| byte == 0)
            .then_some(Address(id.to_address_lossy().0))
            .ok_or(anyhow!(
                "First 12 bytes are not 0, it is not ethereum address"
            ))
    }
}

#[derive(Clone, Encode, Decode, PartialEq, Eq, Hash)]
pub struct Signature(pub [u8; 65]);

pub struct Hash([u8; 32]);

impl From<Hash> for gprimitives::H256 {
    fn from(source: Hash) -> gprimitives::H256 {
        gprimitives::H256::from_slice(&source.0)
    }
}

pub fn hash(data: &[u8]) -> gprimitives::H256 {
    Hash(<[u8; 32]>::from(sha3::Keccak256::digest(data))).into()
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

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl Signature {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl From<[u8; 65]> for Signature {
    fn from(bytes: [u8; 65]) -> Self {
        Self(bytes)
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

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Default for Signature {
    fn default() -> Self {
        Signature([0u8; 65])
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

    pub fn raw_sign_digest(&self, public_key: PublicKey, digest: [u8; 32]) -> Result<Signature> {
        let secret_key = self.get_private_key(public_key)?;

        let secp_secret_key = secp256k1::SecretKey::from_slice(&secret_key.0)
            .with_context(|| "Invalid secret key format for {:?}")?;

        let message = Message::from_digest(digest);

        let recsig =
            secp256k1::global::SECP256K1.sign_ecdsa_recoverable(&message, &secp_secret_key);

        let mut r = Signature::default();
        let (recid, sig) = recsig.serialize_compact();
        r.0[..64].copy_from_slice(&sig);
        r.0[64] = recid.to_i32() as u8;

        Ok(r)
    }

    pub fn sign_digest(&self, public_key: PublicKey, digest: [u8; 32]) -> Result<Signature> {
        let mut r = self.raw_sign_digest(public_key, digest)?;
        r.0[64] += 27;

        Ok(r)
    }

    pub fn sign(&self, public_key: PublicKey, data: &[u8]) -> Result<Signature> {
        self.sign_digest(public_key, sha3::Keccak256::digest(data).into())
    }

    pub fn sign_with_addr(&self, address: Address, data: &[u8]) -> Result<Signature> {
        let keys = self.list_keys()?;

        for key in keys {
            if key.to_address() == address {
                return self.sign(key, data);
            }
        }

        anyhow::bail!("Address not found: {}", address);
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
            anyhow::bail!("Invalid key length: {:?}", bytes);
        }

        buf.copy_from_slice(&bytes);

        Ok(PrivateKey(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ethers::utils::keccak256;

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
        let ethers_sig = ethers::core::types::Signature::try_from(&signature.0[..])
            .expect("failed to parse sig");

        let recovered_address = ethers_sig.recover(hash).expect("Failed to recover address");

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
        let ethers_sig = ethers::core::types::Signature::try_from(&signature.0[..])
            .expect("failed to parse sig");

        let recovered_address = ethers_sig.recover(hash).expect("Failed to recover address");

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
}
