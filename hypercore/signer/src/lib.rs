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

//! Signer library for hypercore.

use anyhow::{Context as _, Result};
use secp256k1::{
    ecdsa::RecoverableSignature,
    hashes::{sha256, Hash},
    Message,
};
use sha3::Digest as _;
use std::{fmt, fs, path::PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct PublicKey(pub(crate) [u8; 33]);

pub struct PrivateKey(pub [u8; 32]);

#[derive(Debug, Clone)]
pub struct Signature([u8; 65]);

impl PublicKey {
    pub fn from_bytes(bytes: [u8; 33]) -> Self {
        Self(bytes)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn to_address(&self) -> String {
        let hash = sha3::Keccak256::digest(&self.0[1..]);
        let address = hex::encode(&hash[12..32]);
        format!("0x{}", address)
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = match hex::decode(s) {
            Ok(bytes) => bytes,
            _ => anyhow::bail!("Invalid hex format for {:?}", s),
        };

        let mut buf = [0u8; 33];
        buf.copy_from_slice(&bytes);
        Ok(Self(buf))
    }
}

impl Signature {
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_hex())
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

impl From<RecoverableSignature> for Signature {
    fn from(recsig: RecoverableSignature) -> Signature {
        let mut r = Self::default();
        let (recid, sig) = recsig.serialize_compact();
        r.0[..64].copy_from_slice(&sig);
        r.0[64] = recid.to_i32() as u8;
        r
    }
}

#[derive(Debug)]
pub struct Signer {
    key_store: PathBuf,
}

impl Signer {
    pub fn new(key_store: PathBuf) -> Result<Self> {
        fs::create_dir_all(key_store.as_path())?;

        Ok(Self { key_store })
    }

    pub fn sign(&self, public_key: PublicKey, data: &[u8]) -> Result<Signature> {
        let secret_key = self.get_key(public_key)?;

        let secp_secret_key = secp256k1::SecretKey::from_slice(&secret_key.0)
            .with_context(|| "Invalid secret key format for {:?}")?;

        let digest = sha3::Keccak256::digest(data);
        let message = Message::from_digest(digest.into());

        let signature =
            secp256k1::global::SECP256K1.sign_ecdsa_recoverable(&message, &secp_secret_key);

        Ok(signature.into())
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
            let key = PublicKey::from_hex(file_name.to_string_lossy().as_ref())?;
            keys.push(key);
        }

        Ok(keys)
    }

    fn get_key(&self, key: PublicKey) -> Result<PrivateKey> {
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

    use secp256k1::{ecdsa::Signature, generate_keypair, rand::rngs::OsRng};

    #[test]
    fn test_signer() {
        let key_store = PathBuf::from("/tmp/key-store");
        let signer = Signer::new(key_store).expect("Failed to create signer");

        let (secp_secret, _) = generate_keypair(&mut OsRng);
        let public_key = signer
            .add_key(PrivateKey(secp_secret.secret_bytes()))
            .unwrap();

        assert!(signer.has_key(public_key).unwrap());

        let data = b"hello world";
        let signature = signer.sign(public_key, data).unwrap();

        let secret_key = signer.get_key(public_key).unwrap();
        let secp_secret_key = secp256k1::SecretKey::from_slice(&secret_key.0).unwrap();
        let secp_public_key = secp256k1::PublicKey::from_secret_key_global(&secp_secret_key);

        let message = Message::from_digest(sha3::Keccak256::digest(data).into());
        let signature = Signature::from_der(&signature).unwrap();

        assert!(signature.verify(&message, &secp_public_key).is_ok());
    }
}
